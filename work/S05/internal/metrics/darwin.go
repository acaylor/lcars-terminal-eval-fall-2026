//go:build darwin

package metrics

import (
	"os"
	"runtime"
	"syscall"
	"time"
	"unsafe"

	"golang.org/x/net/route"
	"golang.org/x/sys/unix"
)

var _ = route.RIBTypeInterface

type darwinSampler struct{}

func NewSampler() Sampler { return darwinSampler{} }

func (darwinSampler) Close() {}

func (darwinSampler) Sample() Snapshot {
	snap := Snapshot{At: time.Now(), OS: "darwin", Arch: runtime.GOARCH}
	if h, err := os.Hostname(); err == nil {
		snap.Hostname = Sanitize(h)
	}
	snap.Uptime = darwinUptime()
	snap.CPUCounters, snap.CoreCounters = darwinCPU()
	if l, ok := darwinLoad(); ok {
		snap.Load, snap.LoadOK = l, true
	}
	if m, ok := darwinMem(); ok {
		snap.Mem, snap.MemOK = m, true
	}
	if used, total, free, ok := darwinStatfs(); ok {
		snap.Disk = Disk{Path: "/", Total: total, Used: used, Free: free}
		snap.DiskOK = true
	}
	snap.Net, snap.NetOK = darwinNet()
	snap.procTicks, snap.procInfoCache, snap.procCount = darwinProcs()
	return snap
}

func sysctlUint64(name string) (uint64, bool) {
	v, err := unix.SysctlUint64(name)
	return v, err == nil
}

func darwinUptime() time.Duration {
	// sysctl kern.boottime -> timeval
	v, err := unix.SysctlTimeval("kern.boottime")
	if err != nil {
		return 0
	}
	up := time.Since(time.Unix(v.Sec, int64(v.Usec)*1000))
	if up < 0 {
		return 0
	}
	return up
}

// darwinCPU reads kern.cp_time — on Darwin this is a long[CPUSTATES]
// array (user nice sys idle intr) in CLK_TCK ticks, via sysctl. Size may
// be reported in longs; tolerate either width. Per-core data is not
// exposed via a stable public sysctl, so cores stay nil (diag: N/A).
func darwinCPU() (total []uint64, cores [][]uint64) {
	raw, err := unix.SysctlRaw("kern.cp_time")
	if err != nil || len(raw) < 40 {
		return nil, nil
	}
	mk := func(sz int) []uint64 {
		if len(raw) < 5*sz {
			return nil
		}
		if sz == 8 {
			return []uint64{
				*(*uint64)(unsafe.Pointer(&raw[0])), *(*uint64)(unsafe.Pointer(&raw[8])),
				*(*uint64)(unsafe.Pointer(&raw[16])), *(*uint64)(unsafe.Pointer(&raw[24])),
				0, *(*uint64)(unsafe.Pointer(&raw[32])), 0, 0,
			}
		}
		g := func(i int) uint64 { return uint64(*(*uint32)(unsafe.Pointer(&raw[i*4]))) }
		return []uint64{g(0), g(1), g(2), g(3), 0, g(4), 0, 0}
	}
	if v := mk(8); v != nil && (v[0]+v[1]+v[2]+v[3]) > 0 {
		return v, nil
	}
	if v := mk(4); v != nil {
		return v, nil
	}
	return nil, nil
}

func darwinLoad() ([3]float64, bool) {
	// sysctl vm.loadavg -> struct loadavg { fixpt_t ldavg[3]; long fscale; }
	raw, err := unix.SysctlRaw("vm.loadavg")
	if err != nil || len(raw) < 16 {
		return [3]float64{}, false
	}
	// loadavg: 3 x uint32 fixed point, then long fscale. Parse with
	// native endianness via unsafe read of uint32s.
	u32 := func(off int) uint32 {
		return *(*uint32)(unsafe.Pointer(&raw[off]))
	}
	fscale := u32(12)
	if fscale == 0 {
		return [3]float64{}, false
	}
	return [3]float64{
		float64(u32(0)) / float64(fscale),
		float64(u32(4)) / float64(fscale),
		float64(u32(8)) / float64(fscale),
	}, true
}

// darwinMem reports hw.memsize (total) and vm.swapusage (swap). macOS
// does not expose Mach vm_statistics64 through sysctl; rather than port
// cgo for one number, Used/Available stay 0 and the view renders
// "N/A" for the split while still showing TOTAL correctly? No — fabricating
// precision is worse. We therefore report MemOK=false for the split but
// stash Total in a way the view can still show. Simplest honest choice:
// MemOK=false, Total set but unused by renderers' Used/Total line.
func darwinMem() (Memory, bool) {
	total, ok := sysctlUint64("hw.memsize")
	if !ok {
		return Memory{}, false
	}
	m := Memory{Total: total}
	if raw, err := unix.SysctlRaw("vm.swapusage"); err == nil && len(raw) >= 24 {
		r64 := func(off int) uint64 { return *(*uint64)(unsafe.Pointer(&raw[off])) }
		m.SwapTotal = r64(0)
		m.SwapFree = r64(8)
		m.SwapAvailable = true
	}
	if m.Total > 0 {
		// Surface total-only info through the disk-style path: keep MemOK
		// false so Used/Total is never fabricated.
		return m, false
	}
	return m, false
}

func darwinStatfs() (used, total, free uint64, ok bool) {
	var st unix.Statfs_t
	if err := unix.Statfs("/", &st); err != nil {
		return 0, 0, 0, false
	}
	total = st.Blocks * uint64(st.Bsize)
	free = st.Bavail * uint64(st.Bsize)
	return total - free, total, free, true
}

// darwinNet reads interface byte counters from NET_RT_IFLIST sysctl
// messages (parsed via x/net/route for names/types, raw offsets for the
// byte counters the exported API does not expose). Loopback is excluded.
// Any layout surprise degrades to N/A (NetOK=false), never a crash.
func darwinNet() (NetTotals, bool) {
	buf, err := route.FetchRIB(syscall.AF_UNSPEC, route.RIBTypeInterface, 0)
	if err != nil || len(buf) == 0 {
		return NetTotals{}, false
	}
	msgs, err := route.ParseRIB(route.RIBTypeInterface, buf)
	if err != nil || len(msgs) == 0 {
		return NetTotals{}, false
	}
	// Names in RIB order match raw message order (same kernel dump).
	names := map[int]bool{} // message index -> is-interface
	idx := 0
	for _, m := range msgs {
		if im, ok := m.(*route.InterfaceMessage); ok && im != nil {
			names[idx] = im.Name != "lo0"
		}
		idx++
	}
	var n NetTotals
	seen := false
	mi := 0
	for off := 0; off+16 <= len(buf); {
		msglen := int(*(*uint16)(unsafe.Pointer(&buf[off])))
		if msglen < 60 || off+msglen > len(buf) {
			break
		}
		if buf[off+3] == unix.RTM_IFINFO || buf[off+3] == unix.RTM_IFINFO2 {
			rx := *(*uint32)(unsafe.Pointer(&buf[off+36]))
			tx := *(*uint32)(unsafe.Pointer(&buf[off+40]))
			if keep, ok := names[mi]; !ok || keep {
				n.Rx += uint64(rx)
				n.Tx += uint64(tx)
				n.Ifaces++
			}
			mi++
			seen = true
		}
		off += msglen
	}
	if !seen {
		return NetTotals{}, false
	}
	return n, true
}

// darwinProcs enumerates processes via kern.proc.all (kinfo_proc).
// Per-process CPU% needs rusage deltas which kern.proc does not carry;
// we use p_pctcpu (fixed-point, <<8) as an instantaneous estimate and
// resident size from e_vm.rssize pages. Processes that vanish mid-scan
// are skipped. This keeps the build cgo-free.
func darwinProcs() (ticks map[int]uint64, info map[int]Process, count int) {
	ticks = map[int]uint64{}
	info = map[int]Process{}
	procs, err := unix.SysctlKinfoProcSlice("kern.proc.all")
	if err != nil {
		return ticks, info, 0
	}
	page := uint64(os.Getpagesize())
	count = len(procs)
	for _, kp := range procs {
		pid := int(kp.Proc.P_pid)
		if pid <= 0 {
			continue
		}
		name := cstr(kp.Proc.P_comm[:])
		rss := uint64(kp.Eproc.Xrssize) * page
		// p_pctcpu is fixpt_t (FSCALE 2048): pct = v*100/2048.
		pct := float64(kp.Proc.P_pctcpu) * 100.0 / 2048.0
		// Encode instantaneous % as ticks over a nominal 1s window so the
		// shared sorter orders diag sensibly even without rusage history.
		ticks[pid] = uint64(pct * 100) // ticks at 100Hz for dt=1s => %
		info[pid] = Process{PID: pid, Name: Sanitize(name), Mem: rss, CPU: pct}
	}
	return ticks, info, count
}

func cstr(b []byte) string {
	for i, c := range b {
		if c == 0 {
			return string(b[:i])
		}
	}
	return string(b)
}
