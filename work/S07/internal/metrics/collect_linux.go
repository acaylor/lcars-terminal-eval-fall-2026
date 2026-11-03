//go:build linux

package metrics

import (
	"bufio"
	"encoding/binary"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

type nativeCollector struct {
	root, host, osName string
	hz                 float64
}

func NewCollector() Collector {
	host, _ := os.Hostname()
	name := "Linux"
	if data, err := os.ReadFile("/etc/os-release"); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			if strings.HasPrefix(line, "PRETTY_NAME=") {
				name = strings.Trim(strings.TrimPrefix(line, "PRETTY_NAME="), "\"")
				break
			}
		}
	}
	return &nativeCollector{root: "/proc", host: host, osName: name, hz: clockTicks()}
}

// AT_CLKTCK comes from the kernel; do not confuse USER_HZ with kernel CONFIG_HZ.
func clockTicks() float64 {
	data, err := os.ReadFile("/proc/self/auxv")
	if err != nil {
		return 0
	}
	word := strconv.IntSize / 8
	for len(data) >= word*2 {
		var tag, value uint64
		if word == 8 {
			tag, value = binary.NativeEndian.Uint64(data), binary.NativeEndian.Uint64(data[word:])
		} else {
			tag, value = uint64(binary.NativeEndian.Uint32(data)), uint64(binary.NativeEndian.Uint32(data[word:]))
		}
		if tag == 17 {
			return float64(value)
		}
		data = data[word*2:]
	}
	return 0
}

func (c *nativeCollector) Collect(details bool) Raw {
	r := Raw{At: time.Now(), Host: c.host, OS: c.osName, Arch: runtime.GOARCH}
	read := func(name string) []byte {
		b, e := os.ReadFile(filepath.Join(c.root, name))
		if e != nil {
			r.Issues = append(r.Issues, name+" unavailable")
		}
		return b
	}
	r.CPU = parseCPU(string(read("stat")))
	if fields := strings.Fields(string(read("uptime"))); len(fields) > 0 {
		if n, e := strconv.ParseFloat(fields[0], 64); e == nil && n >= 0 {
			r.Uptime = time.Duration(n * float64(time.Second))
			r.UptimeOK = true
		}
	}
	if fields := strings.Fields(string(read("loadavg"))); len(fields) >= 3 {
		r.LoadOK = true
		for i := range r.Load {
			n, e := strconv.ParseFloat(fields[i], 64)
			if e != nil {
				r.LoadOK = false
			}
			r.Load[i] = n
		}
	}
	r.Memory, r.Swap = parseMemory(string(read("meminfo")))
	var stat syscall.Statfs_t
	if err := syscall.Statfs("/", &stat); err == nil && stat.Blocks > 0 && stat.Bfree <= stat.Blocks {
		r.Disk = Capacity{Total: stat.Blocks * uint64(stat.Bsize), Used: (stat.Blocks - stat.Bfree) * uint64(stat.Bsize), Available: stat.Bavail * uint64(stat.Bsize), OK: true}
	} else {
		r.Issues = append(r.Issues, "root filesystem unavailable")
	}
	r.Network = parseNetwork(string(read("net/dev")))
	// Interface indices distinguish a recreated interface from its predecessor.
	for name, counter := range r.Network {
		if b, e := os.ReadFile(filepath.Join("/sys/class/net", name, "ifindex")); e == nil {
			delete(r.Network, name)
			r.Network[name+"/"+strings.TrimSpace(string(b))] = counter
		}
	}
	if details {
		entries, err := os.ReadDir(c.root)
		if err == nil {
			r.ProcessesOK = true
			r.Processes = make(map[int]Process)
			for _, entry := range entries {
				pid, e := strconv.Atoi(entry.Name())
				if e != nil || !entry.IsDir() {
					continue
				}
				r.ProcessCount++
				b, e := os.ReadFile(filepath.Join(c.root, entry.Name(), "stat"))
				if e != nil {
					continue
				}
				if p, ok := parseProcess(string(b), c.hz, uint64(os.Getpagesize())); ok {
					r.Processes[pid] = p
				}
			}
		} else {
			r.Issues = append(r.Issues, "processes unavailable")
		}
	}
	return r
}

func parseCPU(data string) map[string]CPUCounter {
	result := make(map[string]CPUCounter)
	scan := bufio.NewScanner(strings.NewReader(data))
	for scan.Scan() {
		f := strings.Fields(scan.Text())
		if len(f) < 5 || !strings.HasPrefix(f[0], "cpu") {
			continue
		}
		var counter CPUCounter
		good := true
		// Guest and guest_nice are already included in user and nice.
		for i := 1; i < len(f) && i <= 8; i++ {
			n, e := strconv.ParseUint(f[i], 10, 64)
			if e != nil {
				good = false
				break
			}
			counter.Total += n
			if i == 4 || i == 5 {
				counter.Idle += n
			}
		}
		if good {
			result[f[0]] = counter
		}
	}
	return result
}

func parseMemory(data string) (Capacity, Capacity) {
	values := map[string]uint64{}
	for _, line := range strings.Split(data, "\n") {
		f := strings.Fields(line)
		if len(f) < 2 {
			continue
		}
		n, e := strconv.ParseUint(f[1], 10, 64)
		if e == nil {
			values[strings.TrimSuffix(f[0], ":")] = n * 1024
		}
	}
	capacity := func(totalKey, availKey string) Capacity {
		t, tok := values[totalKey]
		a, aok := values[availKey]
		if !tok || !aok || a > t {
			return Capacity{}
		}
		return Capacity{Total: t, Used: t - a, Available: a, OK: true}
	}
	return capacity("MemTotal", "MemAvailable"), capacity("SwapTotal", "SwapFree")
}

func parseNetwork(data string) map[string]NetCounter {
	result := make(map[string]NetCounter)
	for _, line := range strings.Split(data, "\n") {
		name, tail, ok := strings.Cut(line, ":")
		name = strings.TrimSpace(name)
		if !ok || name == "lo" {
			continue
		}
		f := strings.Fields(tail)
		if len(f) < 16 {
			continue
		}
		rx, e1 := strconv.ParseUint(f[0], 10, 64)
		tx, e2 := strconv.ParseUint(f[8], 10, 64)
		if e1 == nil && e2 == nil {
			result[name] = NetCounter{rx, tx}
		}
	}
	return result
}

func parseProcess(data string, hz float64, pageSize uint64) (Process, bool) {
	left, right := strings.IndexByte(data, '('), strings.LastIndexByte(data, ')')
	if left < 1 || right <= left || hz <= 0 {
		return Process{}, false
	}
	pid, e := strconv.Atoi(strings.TrimSpace(data[:left]))
	if e != nil {
		return Process{}, false
	}
	f := strings.Fields(data[right+1:])
	if len(f) < 22 {
		return Process{}, false
	}
	get := func(i int) (uint64, error) { return strconv.ParseUint(f[i], 10, 64) }
	u, e1 := get(11)
	s, e2 := get(12)
	start, e3 := get(19)
	rss, e4 := get(21)
	if e1 != nil || e2 != nil || e3 != nil || e4 != nil {
		return Process{}, false
	}
	return Process{PID: pid, Name: strings.ToValidUTF8(data[left+1:right], "?"), Start: start, Seconds: float64(u+s) / hz, Memory: rss * pageSize}, true
}
