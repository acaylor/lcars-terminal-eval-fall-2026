//go:build linux

package metrics

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestProcParsers(t *testing.T) {
	cpu := parseCPUs("cpu 10 2 3 20 5 1 1 2 99 99\ncpu0 10 2 3 20 5 1 1 2 99 99\nctxt 123\n")
	if len(cpu) != 2 || cpu[0].Total != 44 || cpu[0].Busy != 19 {
		t.Fatalf("guest counted twice: %+v", cpu)
	}
	m := parseMemory("MemTotal: 100 kB\nMemAvailable: 40 kB\nbroken\nSwapTotal: 0 kB")
	if m["MemTotal"] != 102400 || m["MemAvailable"] != 40960 {
		t.Fatal(m)
	}
	net := parseNetwork("lo: 100 0 0 0 0 0 0 0 200 0 0 0 0 0 0 0\neth0: 20 0 0 0 0 0 0 0 30 0 0 0 0 0 0 0\nbad: x")
	if len(net) != 1 || net["eth0"].TX != 30 {
		t.Fatal(net)
	}
	// /proc/stat field 3 is the first field after the final ')' in comm.
	fields := make([]string, 22)
	for i := range fields {
		fields[i] = "0"
	}
	fields[0] = "R"
	fields[11] = "200"
	fields[12] = "50"
	fields[19] = "999"
	fields[21] = "10"
	p, ok := parseProcess("42 (worker ) λ name) "+strings.Join(fields, " "), 100, 4096)
	if !ok || p.PID != 42 || p.Name != "worker ) λ name" || p.CPUSeconds != 2.5 || p.Memory != 40960 || p.Identity != 999 {
		t.Fatalf("%+v %v", p, ok)
	}
	for _, bad := range []string{"", "42 no parens", "42 (name) R"} {
		if _, ok := parseProcess(bad, 100, 4096); ok {
			t.Fatal("accepted malformed process")
		}
	}
}
func TestCollectorFailureIsolation(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "loadavg"), []byte("1.5 2.5 3.5 1/2 3"), 0600); err != nil {
		t.Fatal(err)
	}
	c := nativeCollector{root: root, host: "fixture", ticks: 100}
	r := c.Collect(true)
	if r.Host != "fixture" || r.Load1.N != 1.5 || r.MemTotal.OK || len(r.Issues) == 0 || !r.ProcessCount.OK {
		t.Fatalf("failed collector poisoned valid metrics: %+v", r)
	}
}
func TestLiveLinuxCollector(t *testing.T) {
	r := NewCollector().Collect(true)
	if !r.MemTotal.OK || !r.Uptime.OK || len(r.CPUs) == 0 || !r.ProcessCount.OK || len(r.Processes) == 0 {
		t.Fatalf("core metrics unavailable: %+v", r.Issues)
	}
}
