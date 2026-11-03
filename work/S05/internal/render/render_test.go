package render

import (
	"bytes"
	"strings"
	"testing"
)

func TestRGBTo256(t *testing.T) {
	if got := rgbTo256(RGB{255, 0, 0}); got != 196 {
		t.Fatalf("red -> %d want 196", got)
	}
	if got := rgbTo256(RGB{0, 0, 0}); got != 16 {
		t.Fatalf("black -> %d want 16", got)
	}
	if got := rgbTo256(RGB{255, 255, 255}); got != 231 && got != 255 {
		t.Fatalf("white -> %d", got)
	}
}

func TestRendererDiff(t *testing.T) {
	var buf bytes.Buffer
	r := NewRenderer(&buf, Mode256)
	b1 := NewBuffer(5, 2, Cell{Ch: ' ', Fg: White, Bg: Black})
	DrawText(b1, 0, 0, "HI", White, Black)
	r.Render(b1)
	first := buf.Len()
	if first == 0 || !strings.Contains(buf.String(), "\x1b[2J") {
		t.Fatalf("first frame must clear: %q", buf.String())
	}
	buf.Reset()
	r.Render(b1) // unchanged
	if buf.Len() != len("\x1b[2;5H\x1b[0m") && buf.Len() > 40 {
		t.Fatalf("unchanged frame wrote %d bytes", buf.Len())
	}
	b2 := NewBuffer(5, 2, Cell{Ch: ' ', Fg: White, Bg: Black})
	DrawText(b2, 0, 0, "HI!", White, Black)
	buf.Reset()
	r.Render(b2)
	if !strings.Contains(buf.String(), "!") {
		t.Fatalf("diff missing changed cell: %q", buf.String())
	}
	// resize invalidate
	r.Invalidate()
	buf.Reset()
	r.Render(b2)
	if !strings.Contains(buf.String(), "\x1b[2J") {
		t.Fatal("invalidate must full-paint")
	}
}

func TestBigClock(t *testing.T) {
	b := NewBuffer(60, 5, Cell{Ch: ' ', Fg: White, Bg: Black})
	w := DrawBig(b, 0, 0, "14:03:09", White, Black, false)
	if w < 20 {
		t.Fatalf("big clock too narrow: %d", w)
	}
	if !strings.Contains(b.String(), "█") {
		t.Fatal("big glyphs missing blocks")
	}
	ba := NewBuffer(60, 5, Cell{Ch: ' ', Fg: White, Bg: Black})
	DrawBig(ba, 0, 1, "12:00", White, Black, true)
	for _, r := range ba.String() {
		if r > 127 {
			t.Fatalf("ascii font emitted %U", r)
		}
	}
}

func TestTrace(t *testing.T) {
	b := NewBuffer(10, 3, Cell{Ch: ' ', Fg: White, Bg: Black})
	DrawTrace(b, 0, 0, 10, 3, []float64{0, 0.5, 1}, White, Black, false)
	found := false
	for _, r := range b.String() {
		if r >= 0x2800 && r <= 0x28FF {
			found = true
		}
	}
	if !found {
		t.Fatal("braille trace missing")
	}
	ba := NewBuffer(10, 2, Cell{Ch: ' ', Fg: White, Bg: Black})
	DrawTrace(ba, 0, 0, 10, 2, []float64{0, 0.5, 1}, White, Black, true)
	for _, r := range ba.String() {
		if r > 127 {
			t.Fatalf("ascii trace emitted %U", r)
		}
	}
}

func TestShapesClipAndBuffer(t *testing.T) {
	th := DefaultTheme()
	b := NewBuffer(10, 4, Cell{Ch: ' ', Fg: th.Fg, Bg: th.Bg})
	RailH(b, 0, 0, 5, 1, Orange, th)
	if b.Get(0, 0).Ch != '█' {
		t.Fatal("rail not drawn")
	}
	b.Set(-1, 99, Cell{Ch: 'x'}) // must not panic
	if got := clip("abcdefgh", 4); got != "abc…" {
		t.Fatalf("clip: %q", got)
	}
	if got := clip("ab", 4); got != "ab" {
		t.Fatalf("clip short: %q", got)
	}
	tha := th.ForMode(Mode256, true)
	ba := NewBuffer(10, 4, Cell{Ch: ' ', Fg: th.Fg, Bg: th.Bg})
	RailH(ba, 0, 0, 5, 1, Orange, tha)
	if ba.Get(0, 0).Ch != '#' {
		t.Fatal("ascii rail must be #")
	}
}
