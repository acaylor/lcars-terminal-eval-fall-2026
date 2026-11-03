// Package ui renders a bounded cell grid without a terminal or OS dependency.
package ui

import (
	"fmt"
	"html"
	"io"
	"strings"
	"unicode"
)

type Color uint32

const (
	Background Color = 0x090c12
	Orange     Color = 0xeea36c
	Amber      Color = 0xffcf87
	Salmon     Color = 0xd88e9f
	Lavender   Color = 0xb9a4dc
	Blue       Color = 0x89b9d4
	White      Color = 0xe7e7ed
	Muted      Color = 0x8592a7
	Dim        Color = 0x283342
)

type Cell struct {
	Rune   rune
	FG, BG Color
}
type Canvas struct {
	W, H  int
	Cells []Cell
	ASCII bool
}

func NewCanvas(w, h int, ascii bool) *Canvas {
	w = max(0, min(w, 400))
	h = max(0, min(h, 160))
	c := &Canvas{W: w, H: h, Cells: make([]Cell, w*h), ASCII: ascii}
	for i := range c.Cells {
		c.Cells[i] = Cell{' ', White, Background}
	}
	return c
}
func (c *Canvas) Put(x, y int, r rune, fg, bg Color) {
	if x < 0 || y < 0 || x >= c.W || y >= c.H {
		return
	}
	if c.ASCII && r > 127 {
		switch r {
		case '╭', '╯':
			r = '/'
		case '╮', '╰':
			r = '\\'
		case '─', '━', '▁', '▂', '▃', '▄', '▀':
			r = '-'
		case '│', '┃':
			r = '|'
		case '█', '▌', '▐':
			r = '#'
		case '●', '◉', '◆':
			r = '*'
		default:
			r = '.'
		}
	}
	c.Cells[y*c.W+x] = Cell{r, fg, bg}
}

// External names never control cursor position. Wide/ambiguous symbols are replaced
// instead of trusting terminal-specific emoji widths; combining marks are omitted.
func SafeText(s string, ascii bool) string {
	var b strings.Builder
	for _, r := range s {
		if unicode.IsControl(r) || unicode.Is(unicode.Cf, r) {
			b.WriteByte('?')
			continue
		}
		if unicode.Is(unicode.M, r) {
			continue
		}
		if r > 0x1fff || (r >= 0x1100 && r <= 0x11ff) || (ascii && r > 127) {
			b.WriteByte('?')
		} else {
			b.WriteRune(r)
		}
	}
	return b.String()
}
func (c *Canvas) Text(x, y int, s string, fg Color) { c.Label(x, y, s, fg, Background) }
func (c *Canvas) Label(x, y int, s string, fg, bg Color) {
	for _, r := range SafeText(s, c.ASCII) {
		c.Put(x, y, r, fg, bg)
		x++
	}
}
func (c *Canvas) ClipText(x, y, width int, s string, fg Color) {
	r := []rune(SafeText(s, c.ASCII))
	if width <= 0 {
		return
	}
	if len(r) > width {
		r = r[:width]
		r[width-1] = '~'
	}
	c.Text(x, y, string(r), fg)
}
func (c *Canvas) Fill(x, y, w, h int, bg Color) {
	for row := max(0, y); row < min(c.H, y+h); row++ {
		for col := max(0, x); col < min(c.W, x+w); col++ {
			c.Put(col, row, ' ', Background, bg)
		}
	}
}
func (c *Canvas) Pill(x, y, w int, color Color, label string) {
	if w < 3 {
		return
	}
	c.Fill(x+1, y, w-2, 1, color)
	c.Put(x, y, '▐', color, Background)
	c.Put(x+w-1, y, '▌', color, Background)
	label = SafeText(label, c.ASCII)
	if len(label) > w-4 {
		label = label[:max(0, w-4)]
	}
	c.Label(x+w-2-len(label), y, label, Background, color)
}
func (c *Canvas) Plain() string {
	var b strings.Builder
	for y := 0; y < c.H; y++ {
		for _, cell := range c.Cells[y*c.W : (y+1)*c.W] {
			b.WriteRune(cell.Rune)
		}
		b.WriteByte('\n')
	}
	return b.String()
}

// SVG is an inspection artifact of the exact terminal cells, not an alternate UI.
func (c *Canvas) SVG(w io.Writer) error {
	var b strings.Builder
	fmt.Fprintf(&b, `<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %d %d"><rect width="100%%" height="100%%" fill="#%06x"/><g font-family="'Noto Sans Mono', 'DejaVu Sans Mono', monospace" font-size="16" xml:space="preserve">`, c.W*10+40, c.H*20+40, c.W*10+40, c.H*20+40, Background)
	for y := 0; y < c.H; y++ {
		for x := 0; x < c.W; x++ {
			cell := c.Cells[y*c.W+x]
			px, py := 20+x*10, 20+y*20
			if cell.BG != Background {
				fmt.Fprintf(&b, `<rect x="%d" y="%d" width="10" height="20" fill="#%06x"/>`, px, py, cell.BG)
			}
			if cell.Rune >= 0x2800 && cell.Rune <= 0x28ff {
				mask := int(cell.Rune - 0x2800)
				bits := [2][4]int{{0, 1, 2, 6}, {3, 4, 5, 7}}
				for col := 0; col < 2; col++ {
					for row := 0; row < 4; row++ {
						if mask&(1<<bits[col][row]) != 0 {
							fmt.Fprintf(&b, `<circle cx="%d" cy="%d" r="1" fill="#%06x"/>`, px+3+col*4, py+3+row*4, cell.FG)
						}
					}
				}
				continue
			}
			quadrants := map[rune]int{'▘': 1, '▝': 2, '▖': 4, '▞': 6, '▛': 7, '▗': 8, '▚': 9, '▜': 11, '▙': 13, '▟': 14}
			if mask, ok := quadrants[cell.Rune]; ok {
				for bit := 0; bit < 4; bit++ {
					if mask&(1<<bit) != 0 {
						fmt.Fprintf(&b, `<rect x="%d" y="%d" width="5" height="10" fill="#%06x"/>`, px+(bit%2)*5, py+(bit/2)*10, cell.FG)
					}
				}
				continue
			}
			// Block glyphs are geometric in a terminal; rectangles avoid font seams in captures.
			dx, dy, dw, dh := 0, 0, 10, 20
			block := true
			switch cell.Rune {
			case '█':
			case '▀':
				dh = 10
			case '▄':
				dy = 10
				dh = 10
			case '▌':
				dw = 5
			case '▐':
				dx = 5
				dw = 5
			default:
				block = false
			}
			if block {
				fmt.Fprintf(&b, `<rect x="%d" y="%d" width="%d" height="%d" fill="#%06x"/>`, px+dx, py+dy, dw, dh, cell.FG)
			} else if cell.Rune != ' ' {
				fmt.Fprintf(&b, `<text x="%d" y="%d" fill="#%06x">%s</text>`, px, py+16, cell.FG, html.EscapeString(string(cell.Rune)))
			}
		}
	}
	b.WriteString("</g></svg>\n")
	_, err := io.WriteString(w, b.String())
	return err
}
