package ui

import (
	bytebuf "bytes"
	"fmt"
	"strings"
	"testing"
	"time"

	"lcars/internal/metrics"
)

func fixture() State {
	s := State{Now: time.Date(2026, 9, 6, 14, 32, 18, 0, time.UTC), Snapshot: metrics.Snapshot{Raw: metrics.Raw{At: time.Now(), Host: "enterprise-engineering", OS: "Test OS", Arch: "arm64", Memory: metrics.Capacity{Total: 32 << 30, Used: 12 << 30, Available: 20 << 30, OK: true}, Disk: metrics.Capacity{Total: 1 << 40, Used: 500 << 30, Available: 524 << 30, OK: true}, Load: [3]float64{1.25, 2.4, 3.5}, LoadOK: true, Uptime: 48 * time.Hour, UptimeOK: true}, Usage: metrics.Value{N: 23.5, OK: true}, Top: []metrics.Process{{PID: 42, Name: "secret-process-name", CPU: metrics.Value{N: 85, OK: true}, Memory: 123456}}}}
	for i := 0; i < 128; i++ {
		s.Snapshot.Cores = append(s.Snapshot.Cores, metrics.Core{ID: fmt.Sprintf("cpu%d", i), Usage: metrics.Value{N: float64(i % 100), OK: true}})
	}
	return s
}
func TestPrimaryComposition(t *testing.T) {
	s := fixture()
	c := Render(120, 36, s)
	text := c.Plain()
	for _, label := range []string{"CHRONOMETRY", "RESOURCE TELEMETRY", "CPU ACTIVE", "available", "RX", "TX", "UP /", "HOST /", "SUN / 06 SEP 2026"} {
		if !strings.Contains(text, label) {
			t.Errorf("missing %q", label)
		}
	}
	if strings.Contains(text, "secret-process-name") || strings.Contains(text, "RESIDENT") {
		t.Fatal("process list leaked into primary")
	}
	empty := 0
	left, right := 0, 0
	for i, cell := range c.Cells {
		if cell.Rune == ' ' && cell.BG == Background {
			empty++
		}
		if cell.BG != Background {
			if i%c.W < 15 {
				left++
			} else if i%c.W > 104 {
				right++
			}
		}
	}
	if float64(empty)/float64(len(c.Cells)) < 0.2 {
		t.Fatal("less than 20% negative space")
	}
	if left < 2*right {
		t.Fatal("silhouette lost its asymmetric spine")
	}
	if got := Diff(c, Render(120, 36, s), TrueColor); got != "" {
		t.Fatal("unchanged frame emits output")
	}
}
func TestLayoutsBoundedAndASCII(t *testing.T) {
	s := fixture()
	s.Snapshot.Host = strings.Repeat("超long\x1b[2J", 40)
	s.Snapshot.Top[0].Name = "café\n\x1b]0;attack\a日本👩‍🚀"
	for _, size := range [][2]int{{0, 0}, {1, 1}, {10, 3}, {59, 19}, {60, 20}, {80, 24}, {104, 31}, {105, 32}, {120, 36}, {160, 50}, {10000, 10000}} {
		for _, diag := range []bool{false, true} {
			for _, ascii := range []bool{false, true} {
				for _, overlay := range []bool{false, true} {
					s.Diagnostics = diag
					s.ASCII = ascii
					s.Help = overlay
					c := Render(size[0], size[1], s)
					if len(c.Cells) != c.W*c.H || c.W > 400 || c.H > 160 {
						t.Fatal("unbounded canvas")
					}
					for _, cell := range c.Cells {
						if cell.Rune < 32 || cell.Rune == 127 {
							t.Fatalf("control leaked: %q", cell.Rune)
						}
						if ascii && cell.Rune > 127 {
							t.Fatal("non-ASCII fallback")
						}
					}
				}
			}
		}
	}
}
func TestDiagnosticsAndCorePaging(t *testing.T) {
	s := fixture()
	s.Diagnostics = true
	for i := 1; i < 5; i++ {
		s.Snapshot.Top = append(s.Snapshot.Top, metrics.Process{PID: 42 + i, Name: fmt.Sprintf("fifth-check-%d", i)})
	}
	for _, size := range [][2]int{{80, 24}, {120, 36}} {
		text := Render(size[0], size[1], s).Plain()
		for _, label := range []string{"SWAP", "PID", "CPU %", "RESIDENT", "secret-process-name", "fifth-check-4"} {
			if !strings.Contains(text, label) {
				t.Fatalf("%v missing %q", size, label)
			}
		}
	}
	a := Render(120, 36, s).Plain()
	s.CorePage = 1
	b := Render(120, 36, s).Plain()
	if a == b || !strings.Contains(a, "PAGE 1/") || !strings.Contains(b, "PAGE 2/") {
		t.Fatal("core pages inaccessible")
	}
}
func TestDiffAndColorFallbacks(t *testing.T) {
	a := NewCanvas(80, 24, false)
	b := NewCanvas(80, 24, false)
	b.Text(10, 4, "X", Orange)
	for _, mode := range []ColorMode{TrueColor, Indexed, Basic, Mono} {
		diff := Diff(a, b, mode)
		if !strings.Contains(diff, "\x1b[5;11H") || !strings.Contains(diff, "X") || strings.Contains(diff, "[2J") || len(diff) > 90 {
			t.Fatalf("bad cell delta %q", diff)
		}
	}
	for _, tc := range []struct {
		term, ct string
		no       bool
		want     ColorMode
	}{{"xterm-256color", "truecolor", false, TrueColor}, {"xterm-256color", "", false, Indexed}, {"xterm", "", false, Basic}, {"xterm", "truecolor", true, Mono}} {
		if got := DetectColor("auto", tc.term, tc.ct, tc.no); got != tc.want {
			t.Fatal(got)
		}
	}
	var bsvg bytebuf.Buffer
	a.Text(0, 0, "<&>", White)
	if err := a.SVG(&bsvg); err != nil || !strings.Contains(bsvg.String(), "&lt;") {
		t.Fatal("invalid escaped SVG")
	}
}
func TestMissingValuesAndSafeNames(t *testing.T) {
	s := State{Now: time.Now()}
	text := Render(120, 36, s).Plain()
	if !strings.Contains(text, "unavailable") || !strings.Contains(text, "--") {
		t.Fatal("missing metrics fabricated")
	}
	if got := SafeText("café日本\x1b\n", false); got != "café????" {
		t.Fatal(got)
	}
}
func BenchmarkRender120x36(b *testing.B) {
	s := fixture()
	for i := 0; i < b.N; i++ {
		Render(120, 36, s)
	}
}
func BenchmarkUnchangedDiff(b *testing.B) {
	c := Render(120, 36, fixture())
	for i := 0; i < b.N; i++ {
		Diff(c, c, TrueColor)
	}
}
