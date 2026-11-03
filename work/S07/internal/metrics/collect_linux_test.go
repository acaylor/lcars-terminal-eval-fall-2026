//go:build linux

package metrics

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLinuxCPUAndMemoryParsing(t *testing.T) {
	cpu := parseCPU("cpu 100 20 30 400 50 6 7 8 90 10\ncpu0 10 2 3 40 5 0 0 0\ncpu1 damaged\nintr 8\n")
	if cpu["cpu"] != (CPUCounter{621, 450}) || len(cpu) != 2 {
		t.Fatal(cpu)
	}
	mem, swap := parseMemory("MemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n")
	if !mem.OK || mem.Used != 750*1024 || mem.Available != 250*1024 || !swap.OK || swap.Total != 0 {
		t.Fatal(mem, swap)
	}
	mem, _ = parseMemory("MemTotal: 100 kB\nMemAvailable: 200 kB\n")
	if mem.OK {
		t.Fatal("invalid capacity")
	}
	mem, _ = parseMemory("MemTotal: 100 kB\n")
	if mem.OK {
		t.Fatal("missing availability must not look like 100% usage")
	}
}
func TestLinuxNetworkParsing(t *testing.T) {
	nets := parseNetwork("lo: 100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0\neth0: 123 0 0 0 0 0 0 0 456 0 0 0 0 0 0 0\nbad: 1 2\n")
	if len(nets) != 1 || nets["eth0"] != (NetCounter{123, 456}) {
		t.Fatal(nets)
	}
}
func processLine(name string) string {
	f := strings.Fields("S 0 0 0 0 0 0 0 0 0 0 120 30 0 0 0 0 0 0 999 1000000 42")
	return fmt.Sprintf("123 (%s) %s", name, strings.Join(f, " "))
}
func TestLinuxProcessNamesAndUnits(t *testing.T) {
	name := "worker ) (café 日本)"
	p, ok := parseProcess(processLine(name), 100, 4096)
	if !ok || p.PID != 123 || p.Name != name || p.Seconds != 1.5 || p.Start != 999 || p.Memory != 42*4096 {
		t.Fatal(p, ok)
	}
	for _, line := range []string{"", "123 no parentheses", "123 (hi) too short", processLine("hi")[:20]} {
		if _, ok := parseProcess(line, 100, 4096); ok {
			t.Fatal(line)
		}
	}
	if _, ok := parseProcess(processLine("hi"), 0, 4096); ok {
		t.Fatal("unknown clock frequency")
	}
}
func TestFailedCollectorIsolation(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "uptime"), []byte("123.5 456"), 0600); err != nil {
		t.Fatal(err)
	}
	c := nativeCollector{root: dir, hz: 100}
	r := c.Collect(true)
	if !r.UptimeOK || r.Uptime.Seconds() != 123.5 || r.Memory.OK || r.LoadOK || len(r.Issues) < 3 {
		t.Fatalf("failure escaped its collector: %+v", r)
	}
}
func TestNativeLinuxSmoke(t *testing.T) {
	c := NewCollector()
	r := c.Collect(true)
	if !r.UptimeOK || !r.Memory.OK || len(r.CPU) == 0 || !r.ProcessesOK || len(r.Processes) == 0 {
		t.Fatalf("core host metrics unavailable: %+v", r.Issues)
	}
	if clockTicks() <= 0 {
		t.Fatal("AT_CLKTCK unavailable")
	}
}
func FuzzProcessParser(f *testing.F) {
	f.Add(processLine("normal"))
	f.Add("123 (a ) b) S")
	f.Fuzz(func(t *testing.T, input string) { parseProcess(input, 100, 4096) })
}
