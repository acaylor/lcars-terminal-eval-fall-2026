package terminal

import (
	"fmt"
	"io"
	"math"
	"strings"

	"starfleet/internal/ui"
)

type Renderer struct {
	previous *ui.Canvas
	Mode     ColorMode
}

func colorCode(color ui.Color, bg bool, mode ColorMode) string {
	base := 38
	if bg {
		base = 48
	}
	if mode == TrueColor {
		v := ui.RGB[color]
		return fmt.Sprintf("%d;2;%d;%d;%d", base, v[0], v[1], v[2])
	}
	if mode == Indexed {
		if color == ui.Background {
			return fmt.Sprintf("%d;5;233", base)
		}
		v := ui.RGB[color]
		index := 16 + 36*int(math.Round(float64(v[0])/255*5)) + 6*int(math.Round(float64(v[1])/255*5)) + int(math.Round(float64(v[2])/255*5))
		return fmt.Sprintf("%d;5;%d", base, index)
	}
	if mode == Mono {
		if bg {
			if color == ui.Background {
				return "40"
			}
			return "47"
		}
		if color == ui.Background {
			return "30"
		}
		return "37"
	}
	n := ui.ANSI[color]
	code := 30 + n
	if n > 7 {
		code = 90 + n - 8
	}
	if bg {
		code += 10
	}
	return fmt.Sprint(code)
}
func Style(cell ui.Cell, mode ColorMode) string {
	return "\x1b[" + colorCode(cell.FG, false, mode) + ";" + colorCode(cell.BG, true, mode) + "m"
}
func (r *Renderer) Render(w io.Writer, c *ui.Canvas) error {
	var b strings.Builder
	full := r.previous == nil || r.previous.W != c.W || r.previous.H != c.H
	if full {
		b.WriteString("\x1b[2J")
	}
	for y := 0; y < c.H; y++ {
		for x := 0; x < c.W; {
			cell := c.Cells[y*c.W+x]
			if !full && cell == r.previous.Cells[y*c.W+x] {
				x++
				continue
			}
			fmt.Fprintf(&b, "\x1b[%d;%dH", y+1, x+1)
			b.WriteString(Style(cell, r.Mode))
			for x < c.W {
				current := c.Cells[y*c.W+x]
				if current.FG != cell.FG || current.BG != cell.BG {
					break
				}
				if !full && current == r.previous.Cells[y*c.W+x] {
					break
				}
				b.WriteRune(current.Ch)
				x++
			}
		}
	}
	if b.Len() > 0 {
		b.WriteString("\x1b[0m")
	}
	if _, err := io.WriteString(w, b.String()); err != nil {
		return err
	}
	r.previous = c
	return nil
}
