package terminal

import (
	"bytes"
	"os"
	"strings"
	"testing"

	"starfleet/internal/ui"
)

func TestCellDiffAndResize(t *testing.T) {
	r := Renderer{Mode: TrueColor}
	a := ui.NewCanvas(80, 24, false)
	a.Write(2, 2, "STARFLEET", ui.Amber)
	var out bytes.Buffer
	if err := r.Render(&out, a); err != nil {
		t.Fatal(err)
	}
	initial := out.Len()
	out.Reset()
	b := ui.NewCanvas(80, 24, false)
	b.Write(2, 2, "STARFLEET", ui.Amber)
	if err := r.Render(&out, b); err != nil {
		t.Fatal(err)
	}
	if out.Len() != 0 {
		t.Fatal("idle frame wrote terminal output")
	}
	b = ui.NewCanvas(80, 24, false)
	b.Write(2, 2, "STARFLEEX", ui.Amber)
	if err := r.Render(&out, b); err != nil {
		t.Fatal(err)
	}
	if out.Len() > initial/10 || strings.Contains(out.String(), "\x1b[2J") {
		t.Fatal("full repaint for one cell")
	}
	out.Reset()
	if err := r.Render(&out, ui.NewCanvas(30, 10, true)); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out.String(), "\x1b[2J") {
		t.Fatal("resize did not reset stale frame")
	}
}
func TestReducedColor(t *testing.T) {
	cell := ui.Cell{Ch: ' ', FG: ui.Text, BG: ui.Orange}
	for _, mode := range []ColorMode{Mono, Basic, Indexed, TrueColor} {
		s := Style(cell, mode)
		if !strings.HasPrefix(s, "\x1b[") {
			t.Fatal(s)
		}
		if mode != TrueColor && strings.Contains(s, ";2;") {
			t.Fatal("truecolor leaked into fallback")
		}
	}
	t.Setenv("NO_COLOR", "test")
	_ = os.Unsetenv("NO_COLOR")
	t.Setenv("TERM", "xterm-256color")
	t.Setenv("COLORTERM", "truecolor")
	if DetectColor("auto") != TrueColor {
		t.Fatal("truecolor detection")
	}
	t.Setenv("NO_COLOR", "")
	if DetectColor("auto") != Mono {
		t.Fatal("NO_COLOR")
	}
	if DetectColor("basic") != Basic {
		t.Fatal("explicit override")
	}
}
