//go:build linux

package metrics

import (
	"encoding/binary"
	"fmt"
	"os"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

type nativeCollector struct {
	root  string
	host  string
	ticks float64
}

func NewCollector() Collector {
	host, _ := os.Hostname()
	return &nativeCollector{root: "/proc", host: host, ticks: clockTicks()}
}
func clockTicks() float64 {
	// AT_CLKTCK comes from the kernel auxiliary vector, avoiding a guessed HZ.
	b, err := os.ReadFile("/proc/self/auxv")
	if err != nil {
		return 0
	}
	word := strconv.IntSize / 8
	read := func(b []byte) uint64 {
		if word == 8 {
			return binary.NativeEndian.Uint64(b)
		}
		return uint64(binary.NativeEndian.Uint32(b))
	}
	for i := 0; i+2*word <= len(b); i += 2 * word {
		if read(b[i:]) == 17 {
			return float64(read(b[i+word:]))
		}
	}
	return 0
}
func number(s string) (float64, bool) { v, e := strconv.ParseFloat(s, 64); return v, e == nil }
func (c *nativeCollector) Collect(processes bool) Raw {
	r := Raw{At: time.Now(), Host: c.host, OS: "Linux", Arch: runtime.GOARCH}
	read := func(name string) string {
		b, e := os.ReadFile(c.root + "/" + name)
		if e != nil {
			r.Issues = append(r.Issues, name)
			return ""
		}
		return string(b)
	}
	if f := strings.Fields(read("uptime")); len(f) > 0 {
		if v, ok := number(f[0]); ok {
			r.Uptime = Known(v)
		}
	}
	if f := strings.Fields(read("loadavg")); len(f) >= 3 {
		values := []*Value{&r.Load1, &r.Load5, &r.Load15}
		for i, p := range values {
			if v, ok := number(f[i]); ok {
				*p = Known(v)
			}
		}
	}
	r.CPUs = parseCPUs(read("stat"))
	m := parseMemory(read("meminfo"))
	if v, ok := m["MemTotal"]; ok {
		r.MemTotal = Known(v)
	}
	if v, ok := m["MemAvailable"]; ok {
		r.MemAvailable = Known(v)
	}
	if v, ok := m["SwapTotal"]; ok {
		r.SwapTotal = Known(v)
		if free, ok := m["SwapFree"]; ok {
			r.SwapUsed = Known(max(0, v-free))
		}
	}
	var fs syscall.Statfs_t
	if e := syscall.Statfs("/", &fs); e == nil {
		r.DiskTotal = Known(float64(fs.Blocks) * float64(fs.Bsize))
		r.DiskUsed = Known(float64(fs.Blocks-fs.Bfree) * float64(fs.Bsize))
	} else {
		r.Issues = append(r.Issues, "filesystem")
	}
	r.Network = parseNetwork(read("net/dev"))
	if processes {
		entries, e := os.ReadDir(c.root)
		if e != nil {
			r.Issues = append(r.Issues, "processes")
		} else {
			count := 0
			for _, entry := range entries {
				pid, e := strconv.Atoi(entry.Name())
				if e != nil || !entry.IsDir() {
					continue
				}
				count++
				b, e := os.ReadFile(fmt.Sprintf("%s/%d/stat", c.root, pid))
				if e != nil {
					continue
				}
				if p, ok := parseProcess(string(b), c.ticks, os.Getpagesize()); ok {
					r.Processes = append(r.Processes, p)
				}
			}
			r.ProcessCount = Known(float64(count))
		}
	}
	return r
}
func parseCPUs(s string) []CPU {
	var cpus []CPU
	for _, line := range strings.Split(s, "\n") {
		f := strings.Fields(line)
		if len(f) < 5 || !strings.HasPrefix(f[0], "cpu") {
			continue
		}
		var total, idle uint64
		valid := true
		// guest and guest_nice are already included in user/nice.
		for i := 1; i < len(f) && i <= 8; i++ {
			n, e := strconv.ParseUint(f[i], 10, 64)
			if e != nil {
				valid = false
				break
			}
			total += n
			if i == 4 || i == 5 {
				idle += n
			}
		}
		if valid && total >= idle {
			cpus = append(cpus, CPU{total - idle, total})
		}
	}
	return cpus
}
func parseMemory(s string) map[string]float64 {
	m := map[string]float64{}
	for _, line := range strings.Split(s, "\n") {
		f := strings.Fields(line)
		if len(f) < 2 {
			continue
		}
		if v, ok := number(f[1]); ok && v >= 0 {
			if len(f) > 2 && f[2] == "kB" {
				v *= 1024
			}
			m[strings.TrimSuffix(f[0], ":")] = v
		}
	}
	return m
}
func parseNetwork(s string) map[string]Network {
	if s == "" {
		return nil
	}
	m := map[string]Network{}
	for _, line := range strings.Split(s, "\n") {
		name, data, ok := strings.Cut(line, ":")
		if !ok {
			continue
		}
		name = strings.TrimSpace(name)
		if name == "lo" {
			continue
		}
		f := strings.Fields(data)
		if len(f) < 16 {
			continue
		}
		rx, e1 := strconv.ParseUint(f[0], 10, 64)
		tx, e2 := strconv.ParseUint(f[8], 10, 64)
		if e1 == nil && e2 == nil {
			m[name] = Network{rx, tx}
		}
	}
	return m
}
func parseProcess(s string, ticks float64, page int) (Process, bool) {
	start, end := strings.Index(s, "("), strings.LastIndex(s, ")")
	if start < 1 || end <= start || ticks <= 0 {
		return Process{}, false
	}
	pid, e := strconv.Atoi(strings.TrimSpace(s[:start]))
	if e != nil {
		return Process{}, false
	}
	f := strings.Fields(s[end+1:])
	if len(f) < 22 {
		return Process{}, false
	}
	u, e1 := strconv.ParseUint(f[11], 10, 64)
	sys, e2 := strconv.ParseUint(f[12], 10, 64)
	born, e3 := strconv.ParseUint(f[19], 10, 64)
	rss, e4 := strconv.ParseInt(f[21], 10, 64)
	if e1 != nil || e2 != nil || e3 != nil || e4 != nil {
		return Process{}, false
	}
	return Process{PID: pid, Name: s[start+1 : end], CPUSeconds: (float64(u) + float64(sys)) / ticks, Memory: uint64(max(0, rss)) * uint64(page), Identity: born, MemoryOK: true}, true
}
