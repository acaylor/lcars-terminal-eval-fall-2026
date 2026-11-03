//go:build linux

package metrics

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func readTestdata(t *testing.T, name string) string {
	t.Helper()
	b, err := os.ReadFile(filepath.Join("..", "..", "testdata", name))
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	return string(b)
}

func TestParseStat(t *testing.T) {
	total, cores := parseStat(readTestdata(t, "stat.txt"))
	if len(total) < 4 {
		t.Fatalf("total counters: %v", total)
	}
	if len(cores) == 0 || len(cores) != runtime.NumCPU() && false {
		t.Fatalf("cores: %d", len(cores))
	}
	if len(cores) < 1 {
		t.Fatalf("expected >=1 core, got %d", len(cores))
	}
}

func TestCPUBusyPct(t *testing.T) {
	prev := []uint64{100, 0, 100, 800, 0, 0, 0, 0}
	cur := []uint64{150, 0, 150, 1700, 0, 0, 0, 0}
	p, ok := cpuBusyPct(prev, cur)
	if !ok || p != 10 {
		t.Fatalf("got %v %v want 10 true", p, ok)
	}
	if _, ok := cpuBusyPct(cur, prev); ok {
		t.Fatal("counter reset must report not-ok")
	}
	if _, ok := cpuBusyPct(prev, prev); ok {
		t.Fatal("zero delta must report not-ok")
	}
	if _, ok := cpuBusyPct(prev[:2], cur); ok {
		t.Fatal("short slices must report not-ok")
	}
}

func TestParseMeminfo(t *testing.T) {
	m, ok := parseMeminfo(readTestdata(t, "meminfo.txt"))
	if !ok || m.Total == 0 || m.Available == 0 || m.Available > m.Total {
		t.Fatalf("bad meminfo: %+v ok=%v", m, ok)
	}
	if m.Used+m.Available != m.Total {
		t.Fatalf("used+avail != total: %+v", m)
	}
	if _, ok := parseMeminfo("garbage\n"); ok {
		t.Fatal("garbage must fail")
	}
}

func TestParseLoadavgUptime(t *testing.T) {
	if l, ok := parseLoadavg(readTestdata(t, "loadavg.txt")); !ok || l[0] < 0 {
		t.Fatalf("loadavg: %v %v", l, ok)
	}
	if _, ok := parseLoadavg("x y"); ok {
		t.Fatal("short loadavg must fail")
	}
	if v, ok := parseUptime(readTestdata(t, "uptime.txt")); !ok || v <= 0 {
		t.Fatalf("uptime: %v %v", v, ok)
	}
}

func TestParseNetDev(t *testing.T) {
	n, ok := parseNetDev(readTestdata(t, "netdev.txt"))
	if !ok || n.Ifaces < 1 || n.Rx == 0 {
		t.Fatalf("netdev: %+v %v", n, ok)
	}
	if _, ok := parseNetDev("lo: 1 2 3\n"); ok {
		t.Fatal("loopback-only must fail")
	}
}

func TestParseProcStatTricky(t *testing.T) {
	// comm with spaces, parens and non-ASCII.
	s := "42 (weird) (na\xcc\x88me) S 1 2 3 4 5 6 7 8 9 10 100 50 0 0 20 0 1 0 999 123456 200 0 0 0"
	name, ppid, ticks, rss, ok := parseProcStat(s)
	if !ok || ppid != 1 || ticks != 150 || rss != 200*uint64(os.Getpagesize()) {
		t.Fatalf("got name=%q ppid=%d ticks=%d rss=%d ok=%v", name, ppid, ticks, rss, ok)
	}
	if name == "" {
		t.Fatal("empty name")
	}
	if _, _, _, _, ok := parseProcStat("no parens here"); ok {
		t.Fatal("malformed must fail")
	}
	// Live sample from fixture.
	name2, _, _, _, ok2 := parseProcStat(readTestdata(t, "procstat.txt"))
	if !ok2 || name2 == "" {
		t.Fatalf("fixture procstat: %q %v", name2, ok2)
	}
}

func TestComputeRates(t *testing.T) {
	t0 := time.Unix(1000, 0)
	prev := Snapshot{At: t0,
		CPUCounters:  []uint64{100, 0, 100, 800, 0, 0, 0, 0},
		CoreCounters: [][]uint64{{100, 0, 100, 800, 0, 0, 0, 0}, {0, 0, 0, 1000, 0, 0, 0, 0}},
		Net:          NetTotals{Rx: 1000, Tx: 2000}, NetOK: true}
	cur := Snapshot{At: t0.Add(2 * time.Second),
		CPUCounters:  []uint64{200, 0, 200, 1600, 0, 0, 0, 0},
		CoreCounters: [][]uint64{{200, 0, 200, 1600, 0, 0, 0, 0}, {0, 0, 0, 3000, 0, 0, 0, 0}},
		Net:          NetTotals{Rx: 3000, Tx: 2000}, NetOK: true}
	r := ComputeRates(prev, cur)
	if !r.CPUPctOK || r.CPUPct != 20 {
		t.Fatalf("cpu: %v %v", r.CPUPct, r.CPUPctOK)
	}
	if len(r.Cores) != 2 || r.Cores[1].Pct != 0 {
		t.Fatalf("cores: %+v", r.Cores)
	}
	if !r.NetOK || r.NetRx != 1000 || r.NetTx != 0 {
		t.Fatalf("net: %+v", r)
	}
	// Zero elapsed: no NaN, net not ok.
	z := ComputeRates(cur, cur)
	if z.Elapsed != 0 || z.NetOK {
		t.Fatalf("zero elapsed: %+v", z)
	}
	// First sample.
	f := ComputeRates(Snapshot{}, cur)
	if f.CPUPctOK {
		t.Fatal("first sample must have no cpu rate")
	}
}

func TestComputeProcessRates(t *testing.T) {
	prev := map[int]uint64{1: 100, 2: 100}
	cur := map[int]uint64{1: 200, 2: 150, 3: 50}
	info := map[int]Process{1: {Name: "a", Mem: 10}, 2: {Name: "b", Mem: 999}, 3: {Name: "c", Mem: 5}}
	out := ComputeProcessRates(prev, cur, info, time.Second, 2)
	if len(out) != 2 || out[0].PID != 1 {
		t.Fatalf("top procs: %+v", out)
	}
	// dt=0 -> all zero CPU, sorted by memory.
	out0 := ComputeProcessRates(prev, cur, info, 0, 5)
	if out0[0].PID != 2 || out0[0].CPU != 0 {
		t.Fatalf("dt0: %+v", out0)
	}
}

func TestSanitizeFormat(t *testing.T) {
	if got := Sanitize("a\x1b[31mb\tc"); got != "a�[31mb�c" {
		t.Fatalf("sanitize: %q", got)
	}
	if UptimeString(25*time.Hour+3*time.Minute) != "1d 01h 03m" {
		t.Fatalf("uptime fmt: %s", UptimeString(25*time.Hour+3*time.Minute))
	}
	if RateString(0) != "0 B/s" || BytesString(1<<30) != "1.0 GB" {
		t.Fatalf("fmt: %s %s", RateString(0), BytesString(1<<30))
	}
}
