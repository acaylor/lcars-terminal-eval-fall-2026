package ui

import (
	bytebuf "bytes"
	"encoding/xml"
	"io"
	"strings"
	"testing"
	"time"
	"unicode/utf8"

	"starfleet/internal/metrics"
)

func fixture() metrics.Snapshot {
	k := metrics.Known
	return metrics.Snapshot{Raw: metrics.Raw{At: time.Date(2026, 9, 30, 12, 34, 56, 0, time.UTC), Host: strings.Repeat("long-host-", 30), OS: "Linux", Arch: "amd64", Uptime: k(123456), Load1: k(1.23), Load5: k(2.34), Load15: k(3.45), MemTotal: k(32 << 30), MemAvailable: k(20 << 30), DiskTotal: k(500 << 30), DiskUsed: k(200 << 30), SwapTotal: k(2 << 30), SwapUsed: k(1 << 30), ProcessCount: k(9)}, CPU: k(27), RX: k(1024), TX: k(2048), Cores: []metrics.Value{k(20), k(34)}, Top: []metrics.ProcessView{{PID: 1234, Name: "process-secret-λ\x1b[2J", CPU: k(12), Memory: k(100 << 20)}}}
}
func TestLayoutsAndFallbacks(t *testing.T) {
	for _, size := range [][2]int{{120, 36}, {80, 24}, {105, 34}, {120, 30}, {1, 1}, {20, 5}, {77, 23}, {500, 200}} {
		for _, diag := range []bool{false, true} {
			for _, help := range []bool{false, true} {
				for _, ascii := range []bool{false, true} {
					c := Draw(size[0], size[1], fixture(), State{Diagnostic: diag, Help: help, ASCII: ascii})
					if len(c.Cells) != c.W*c.H {
						t.Fatal("invalid grid")
					}
					lines := strings.Split(strings.TrimSuffix(c.Plain(), "\n"), "\n")
					if len(lines) != c.H {
						t.Fatal("invalid height")
					}
					for _, line := range lines {
						if utf8.RuneCountInString(line) != c.W {
							t.Fatal("invalid width")
						}
						if ascii {
							for _, r := range line {
								if r > 127 {
									t.Fatal("non-ASCII fallback")
								}
							}
						}
					}
					if strings.Contains(c.Plain(), "\x1b") {
						t.Fatal("terminal injection")
					}
				}
			}
		}
	}
}
func TestCompositionAndViewSeparation(t *testing.T) {
	s := fixture()
	primary := Draw(120, 36, s, State{})
	secondary := Draw(120, 36, s, State{Diagnostic: true})
	if strings.Contains(primary.Plain(), "process-secret") || !strings.Contains(secondary.Plain(), "process-secret") {
		t.Fatal("process view leaked or absent")
	}
	blank, rails, focal := 0, 0, 0
	for _, cell := range primary.Cells {
		if cell.Ch == ' ' && cell.BG == Background {
			blank++
		}
		if cell.BG != Background {
			rails++
		}
		if cell.FG == Amber && cell.Ch != ' ' {
			focal++
		}
	}
	if blank < len(primary.Cells)/5 || rails < 300 || focal < 60 {
		t.Fatalf("composition lost: blank=%d rails=%d focal=%d", blank, rails, focal)
	}
	compact := Draw(80, 24, s, State{}).Plain()
	for _, label := range []string{"LOAD", "MEM", "FREE", "ROOT", "RX", "TX", "CPU", "UP", "56", "TAB VIEW"} {
		if !strings.Contains(compact, label) {
			t.Errorf("80x24 missing %s", label)
		}
	}
	if strings.Contains(primary.Plain(), "COMMAND REFERENCE") || !strings.Contains(Draw(120, 36, s, State{Help: true}).Plain(), "COMMAND REFERENCE") {
		t.Fatal("help view")
	}
}
func TestSafeTextAndSVG(t *testing.T) {
	if Safe("λ\x1b\n\u202e界e\u0301") != "λ?e" {
		t.Fatalf("unsafe text %q", Safe("λ\x1b\n\u202e界e\u0301"))
	}
	c := NewCanvas(4, 2, false)
	c.Write(0, 0, "<&λ", Text)
	var out bytebuf.Buffer
	if err := c.SVG(&out); err != nil {
		t.Fatal(err)
	}
	decoder := xml.NewDecoder(&out)
	for {
		_, err := decoder.Token()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
	}
}
func FuzzRender(f *testing.F) {
	f.Add(uint16(120), uint16(36), "process λ", true)
	f.Fuzz(func(t *testing.T, w, h uint16, name string, diag bool) {
		s := fixture()
		s.Host = name
		s.Top[0].Name = name
		c := Draw(int(w%501), int(h%201), s, State{Diagnostic: diag})
		if len(c.Cells) != c.W*c.H {
			t.Fatal("grid")
		}
	})
}
