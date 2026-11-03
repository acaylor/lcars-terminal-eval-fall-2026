package app

import (
	"strings"
	"testing"
	"time"

	"lcars/internal/metrics"
	"lcars/internal/render"
)

func fakeState(w, h int, view View) *State {
	s := &State{TermW: w, TermH: h, Mode: render.Mode256, CPUScale: 0.5}
	s.Snap = metrics.Snapshot{
		At:       time.Date(2026, 10, 27, 14, 3, 9, 0, time.Local),
		Hostname: "linux.example.org", OS: "linux", Arch: "amd64",
		Uptime: 25*time.Hour + 3*time.Minute,
		Load:   [3]float64{0.42, 0.31, 0.22}, LoadOK: true,
		Mem:   metrics.Memory{Total: 96 << 30, Available: 80 << 30, Used: 16 << 30, SwapTotal: 8 << 30, SwapAvailable: true},
		MemOK: true,
		Disk:  metrics.Disk{Path: "/", Total: 500 << 30, Used: 120 << 30, Free: 380 << 30}, DiskOK: true,
	}
	s.Rates = metrics.Rates{CPUPct: 23.5, CPUPctOK: true, NetRx: 1 << 20, NetTx: 2 << 20, NetOK: true, ProcCount: 312,
		Cores:   []metrics.CPUCore{{Label: "C1", Pct: 10}, {Label: "C2", Pct: 90}},
		Procs:   []metrics.Process{{PID: 1, Name: "init", CPU: 5, Mem: 1 << 20}},
		ProcsOK: true}
	s.HasRates = true
	s.CPURing = []float64{0.1, 0.4, 0.2, 0.9, 0.3}
	s.View = view
	return s
}

func TestLayoutPrimary120x36(t *testing.T) {
	b := Build(fakeState(120, 36, ViewPrimary))
	txt := b.String()
	// Process table header must NOT appear on primary.
	if strings.Contains(txt, "CPU%") || strings.Contains(txt, "NAME ") && strings.Contains(txt, "PID") {
		t.Fatalf("process list leaked onto primary:\n%s", txt)
	}
	if !strings.Contains(txt, "▄█") && !strings.Contains(txt, "▄██▄") {
		t.Fatalf("clock missing:\n%s", txt)
	}
	if !strings.Contains(txt, "LCARS") {
		t.Fatalf("rail label missing:\n%s", txt)
	}
	// Asymmetric silhouette: left rail occupied, right top corner is badge.
	lines := strings.Split(txt, "\n")
	if !strings.Contains(lines[0], "LCARS //") {
		t.Fatalf("header missing: %q", lines[0])
	}
}

func TestDiagShowsProcs(t *testing.T) {
	b := Build(fakeState(120, 36, ViewDiag))
	txt := b.String()
	if !strings.Contains(txt, "DIAGNOSTICS") || !strings.Contains(txt, "init") {
		t.Fatalf("diag missing content:\n%s", txt)
	}
}

func TestCompact80x24NoBigClock(t *testing.T) {
	b := Build(fakeState(80, 24, ViewPrimary))
	txt := b.String()
	if strings.Contains(txt, "▄██▄") || strings.Contains(txt, "▀██▀") {
		t.Fatal("compact must not use big glyphs")
	}
	if !strings.Contains(txt, "14:03:09") {
		t.Fatalf("compact clock missing:\n%s", txt)
	}
	if strings.Contains(txt, "PID") && strings.Contains(txt, "NAME") {
		t.Fatalf("process list leaked onto compact:\n%s", txt)
	}
}

func TestTinySizesNoPanic(t *testing.T) {
	for _, sz := range [][2]int{{1, 1}, {5, 3}, {19, 7}, {40, 10}} {
		b := Build(fakeState(sz[0], sz[1], ViewPrimary))
		if b.W != sz[0] || b.H != sz[1] {
			t.Fatalf("size %v: got %dx%d", sz, b.W, b.H)
		}
		_ = Build(fakeState(sz[0], sz[1], ViewDiag))
	}
}

func TestHelpOverlay(t *testing.T) {
	s := fakeState(120, 36, ViewPrimary)
	s.Help = true
	txt := Build(s).String()
	if !strings.Contains(txt, "HELP") || !strings.Contains(txt, "quit") {
		t.Fatalf("help:\n%s", txt)
	}
}

func TestASCIIFallback(t *testing.T) {
	s := fakeState(120, 36, ViewPrimary)
	s.Ascii = true
	txt := Build(s).String()
	for _, r := range txt {
		if r > 127 {
			t.Fatalf("non-ascii rune %U in ascii mode", r)
		}
	}
	s2 := fakeState(80, 24, ViewPrimary)
	s2.Ascii = true
	txt2 := Build(s2).String()
	for _, r := range txt2 {
		if r > 127 {
			t.Fatalf("compact ascii: non-ascii %U", r)
		}
	}
}

func TestEscLenAndKeys(t *testing.T) {
	if escLen([]byte("\x1b[A")) != 3 {
		t.Fatal("csi up")
	}
	if escLen([]byte("a")) != 0 {
		t.Fatal("plain")
	}
	r := &runner{rd: render.NewRenderer(discard{}, render.Mode256)}
	if quit := r.handleKeys([]byte("q")); !quit {
		t.Fatal("q must quit")
	}
	r2 := &runner{rd: render.NewRenderer(discard{}, render.Mode256)}
	if quit := r2.handleKeys([]byte{0x1b}); !quit {
		t.Fatal("lone Esc must quit")
	}
	r3 := &runner{rd: render.NewRenderer(discard{}, render.Mode256)}
	_ = r3.handleKeys([]byte("p"))
	if r3.state.View != ViewDiag {
		t.Fatal("p must open diag")
	}
	_ = r3.handleKeys([]byte("p"))
	if r3.state.View != ViewPrimary {
		t.Fatal("p must close diag")
	}
	_ = r3.handleKeys([]byte("\t"))
	if r3.state.View != ViewDiag {
		t.Fatal("Tab must cycle")
	}
}

type discard struct{}

func (discard) Write(p []byte) (int, error) { return len(p), nil }

func TestPushRingBounded(t *testing.T) {
	s := &State{CPUScale: 0.1}
	for i := 0; i < 500; i++ {
		s.pushSample()
	}
	if len(s.CPURing) > ringCap {
		t.Fatalf("ring unbounded: %d", len(s.CPURing))
	}
}
