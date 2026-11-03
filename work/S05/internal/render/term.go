// Package render implements a cell-buffer renderer with diff output.
//
// Philosophy: views paint into a Buffer (pure, testable). The Renderer
// compares the new buffer with the previous one and emits only changed
// spans, using CUP (cursor addressing) and SGR for style changes. The
// whole screen is painted once on start and after resize; steady-state
// ticks emit a few hundred bytes.
package render

import (
	"io"
	"strings"
)

type Renderer struct {
	w    io.Writer
	mode ColorMode
	prev *Buffer
}

func NewRenderer(w io.Writer, mode ColorMode) *Renderer {
	return &Renderer{w: w, mode: mode}
}

// Invalidate forces a full repaint on the next Render call (use after
// resize or overlay transitions).
func (r *Renderer) Invalidate() { r.prev = nil }

func (r *Renderer) cup(x, y int) string {
	return "\x1b[" + itoa(y+1) + ";" + itoa(x+1) + "H"
}

func sgrFor(c Cell, mode ColorMode) string {
	var sb strings.Builder
	sb.WriteString("\x1b[0")
	if c.Attr&AttrBold != 0 {
		sb.WriteString(";1")
	}
	if c.Attr&AttrDim != 0 {
		sb.WriteString(";2")
	}
	if c.Attr&AttrReverse != 0 {
		sb.WriteString(";7")
	}
	if c.Attr&AttrUnderline != 0 {
		sb.WriteString(";4")
	}
	sb.WriteString("m")
	if mode != ModeMono {
		writeColor(&sb, c.Fg, false, mode)
		writeColor(&sb, c.Bg, true, mode)
	}
	return sb.String()
}

func writeColor(sb *strings.Builder, col RGB, isBg bool, mode ColorMode) {
	base := "38"
	if isBg {
		base = "48"
	}
	if mode == ModeTrue {
		sb.WriteString("\x1b[" + base + ";2;" + itoa(int(col.R)) + ";" + itoa(int(col.G)) + ";" + itoa(int(col.B)) + "m")
	} else {
		sb.WriteString("\x1b[" + base + ";5;" + itoa(rgbTo256(col)) + "m")
	}
}

// Render diffs buf against the previous frame and writes the update.
func (r *Renderer) Render(buf *Buffer) {
	var out strings.Builder
	full := r.prev == nil || r.prev.W != buf.W || r.prev.H != buf.H
	if full {
		out.WriteString("\x1b[H\x1b[2J")
	} else if r.prev == nil {
		// unreachable; keeps static checks happy
	}
	for y := 0; y < buf.H; y++ {
		r.emitRow(&out, buf, y, full)
	}
	out.WriteString(r.cup(maxi(0, buf.W-1), maxi(0, buf.H-1)))
	out.WriteString("\x1b[0m")
	_, _ = r.w.Write([]byte(out.String()))
	if r.prev == nil || r.prev.W != buf.W || r.prev.H != buf.H {
		r.prev = NewBuffer(buf.W, buf.H, Cell{})
	}
	copy(r.prev.Cells, buf.Cells)
}

func (r *Renderer) emitRow(out *strings.Builder, buf *Buffer, y int, full bool) {
	x := 0
	for x < buf.W {
		cur := buf.Cells[buf.idx(x, y)]
		if !full && cur == r.prev.Cells[r.prev.idx(x, y)] {
			x++
			continue
		}
		end := x
		for end < buf.W {
			c := buf.Cells[buf.idx(end, y)]
			if !full && c == r.prev.Cells[r.prev.idx(end, y)] {
				break
			}
			if c.Fg != cur.Fg || c.Bg != cur.Bg || c.Attr != cur.Attr {
				break
			}
			end++
		}
		out.WriteString(r.cup(x, y))
		out.WriteString(sgrFor(cur, r.mode))
		for i := x; i < end; i++ {
			ch := buf.Cells[buf.idx(i, y)].Ch
			if ch == 0 {
				ch = ' '
			}
			out.WriteRune(ch)
		}
		x = end
	}
}

func maxi(a, b int) int {
	if a > b {
		return a
	}
	return b
}
