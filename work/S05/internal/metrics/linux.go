//go:build linux

package metrics

import (
	"bufio"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

type linuxSampler struct {
	procRoot string // overridable in tests
	prev     Snapshot
	prevInfo map[int]Process
	first    bool
}

func NewSampler() Sampler {
	return &linuxSampler{procRoot: "/proc", first: true}
}

func (s *linuxSampler) Close() {}

func readFile(path string) ([]byte, error) { return os.ReadFile(path) }

func (s *linuxSampler) Sample() Snapshot {
	snap := Snapshot{At: time.Now(), OS: "linux", Arch: runtime.GOARCH}
	if h, err := os.Hostname(); err == nil {
		snap.Hostname = Sanitize(h)
	}
	if b, err := readFile(filepath.Join(s.procRoot, "uptime")); err == nil {
		if f, ok := parseUptime(string(b)); ok {
			snap.Uptime = time.Duration(f * float64(time.Second))
		}
	}
	if b, err := readFile(filepath.Join(s.procRoot, "stat")); err == nil {
		snap.CPUCounters, snap.CoreCounters = parseStat(string(b))
	}
	if b, err := readFile(filepath.Join(s.procRoot, "loadavg")); err == nil {
		if l, ok := parseLoadavg(string(b)); ok {
			snap.Load, snap.LoadOK = l, true
		}
	}
	if b, err := readFile(filepath.Join(s.procRoot, "meminfo")); err == nil {
		if m, ok := parseMeminfo(string(b)); ok {
			snap.Mem, snap.MemOK = m, true
		}
	}
	if _, t, f, ok := statfsRoot(); ok {
		snap.Disk = Disk{Path: "/", Total: t, Used: t - f, Free: f}
		snap.DiskOK = true
	}
	if b, err := readFile(filepath.Join(s.procRoot, "net/dev")); err == nil {
		if n, ok := parseNetDev(string(b)); ok {
			snap.Net, snap.NetOK = n, true
		}
	}
	snap.procTicks, snap.procCount = s.procs()
	// Resolve names/RSS lazily for the top set: parse every pid's stat
	// once more here is wasteful, so procs() already discarded them.
	// Instead, cache info for all current pids (bounded by proc count).
	snap.procInfoCache = map[int]Process{}
	for pid := range snap.procTicks {
		if p, ok := readProcInfo(s.procRoot, pid); ok {
			snap.procInfoCache[pid] = p
		}
	}
	return snap
}

func statfsRoot() (used, total, free uint64, ok bool) {
	var st syscall.Statfs_t
	if err := syscall.Statfs("/", &st); err != nil {
		return 0, 0, 0, false
	}
	total = st.Blocks * uint64(st.Bsize)
	free = st.Bavail * uint64(st.Bsize)
	return total - free, total, free, true
}

func (s *linuxSampler) procs() (map[int]uint64, int) {
	entries, err := os.ReadDir(s.procRoot)
	if err != nil {
		return map[int]uint64{}, 0
	}
	ticks := map[int]uint64{}
	count := 0
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil || pid <= 0 {
			continue
		}
		count++
		b, err := os.ReadFile(filepath.Join(s.procRoot, e.Name(), "stat"))
		if err != nil {
			continue // process vanished mid-scan: normal
		}
		if _, _, t, _, ok := parseProcStat(string(b)); ok {
			ticks[pid] = t
		}
	}
	return ticks, count
}

// --- pure parsers (tested with testdata fixtures) ---

func parseUptime(s string) (float64, bool) {
	f := strings.Fields(s)
	if len(f) < 1 {
		return 0, false
	}
	v, err := strconv.ParseFloat(f[0], 64)
	return v, err == nil
}

func parseStat(s string) (total []uint64, cores [][]uint64) {
	sc := bufio.NewScanner(strings.NewReader(s))
	for sc.Scan() {
		line := sc.Text()
		if !strings.HasPrefix(line, "cpu") {
			continue
		}
		f := strings.Fields(line)
		if len(f) < 5 {
			continue
		}
		vals := make([]uint64, 0, 8)
		for _, x := range f[1:] {
			if len(vals) == 8 {
				break
			}
			if v, err := strconv.ParseUint(x, 10, 64); err == nil {
				vals = append(vals, v)
			}
		}
		if f[0] == "cpu" {
			total = vals
		} else {
			cores = append(cores, vals)
		}
	}
	return total, cores
}

func parseLoadavg(s string) ([3]float64, bool) {
	f := strings.Fields(s)
	if len(f) < 3 {
		return [3]float64{}, false
	}
	var out [3]float64
	for i := 0; i < 3; i++ {
		v, err := strconv.ParseFloat(f[i], 64)
		if err != nil {
			return [3]float64{}, false
		}
		out[i] = v
	}
	return out, true
}

// Meminfo values are KiB.
func parseMeminfo(s string) (Memory, bool) {
	kv := map[string]uint64{}
	sc := bufio.NewScanner(strings.NewReader(s))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) < 2 {
			continue
		}
		k := strings.TrimSuffix(f[0], ":")
		if v, err := strconv.ParseUint(f[1], 10, 64); err == nil {
			kv[k] = v * 1024
		}
	}
	total, okT := kv["MemTotal"]
	avail, okA := kv["MemAvailable"]
	if !okT {
		return Memory{}, false
	}
	if !okA { // very old kernels: approximate
		avail = kv["MemFree"] + kv["Buffers"] + kv["Cached"]
	}
	m := Memory{Total: total, Available: avail}
	if total > avail {
		m.Used = total - avail
	}
	if st, ok := kv["SwapTotal"]; ok {
		m.SwapTotal = st
		m.SwapFree = kv["SwapFree"]
		m.SwapAvailable = true
	}
	return m, true
}

func parseNetDev(s string) (NetTotals, bool) {
	var n NetTotals
	seen := false
	sc := bufio.NewScanner(strings.NewReader(s))
	for sc.Scan() {
		line := sc.Text()
		i := strings.IndexByte(line, ':')
		if i < 0 {
			continue
		}
		name := strings.TrimSpace(line[:i])
		if name == "lo" || name == "" {
			continue
		}
		f := strings.Fields(line[i+1:])
		if len(f) < 16 {
			continue
		}
		rx, err1 := strconv.ParseUint(f[0], 10, 64)
		tx, err2 := strconv.ParseUint(f[8], 10, 64)
		if err1 != nil || err2 != nil {
			continue
		}
		n.Rx += rx
		n.Tx += tx
		n.Ifaces++
		seen = true
	}
	return n, seen
}

// parseProcStat parses /proc/<pid>/stat. comm is parenthesised and may
// contain spaces, parentheses and non-ASCII bytes, so fields are split
// after the LAST ')'.
func parseProcStat(s string) (name string, ppid int, ticks uint64, rss uint64, ok bool) {
	open := strings.IndexByte(s, '(')
	close := strings.LastIndexByte(s, ')')
	if open < 0 || close < 0 || close <= open {
		return "", 0, 0, 0, false
	}
	name = s[open+1 : close]
	rest := strings.Fields(s[close+1:])
	// rest[0]=state, rest[1]=ppid, ... utime is field 14 => rest[11],
	// stime field 15 => rest[12], rss field 24 => rest[21].
	if len(rest) < 22 {
		return "", 0, 0, 0, false
	}
	pp, _ := strconv.Atoi(rest[1])
	ut, _ := strconv.ParseUint(rest[11], 10, 64)
	st, _ := strconv.ParseUint(rest[12], 10, 64)
	pages, _ := strconv.ParseInt(rest[21], 10, 64)
	if pages < 0 {
		pages = 0
	}
	return name, pp, ut + st, uint64(pages) * uint64(os.Getpagesize()), true
}

// procNames reads comm + RSS for a pid; used by the rate stage.
func readProcInfo(root string, pid int) (Process, bool) {
	b, err := os.ReadFile(filepath.Join(root, strconv.Itoa(pid), "stat"))
	if err != nil {
		return Process{}, false
	}
	name, _, _, rss, ok := parseProcStat(string(b))
	if !ok {
		return Process{}, false
	}
	return Process{PID: pid, Name: Sanitize(name), Mem: rss}, true
}
