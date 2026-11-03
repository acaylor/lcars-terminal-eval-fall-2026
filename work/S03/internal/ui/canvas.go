// Package ui draws into a bounded, terminal-independent cell grid.
package ui

import (
	"fmt"
	"html"
	"io"
	"strings"
	"unicode"
)

type Color uint8

const (
	Background Color = iota
	Text
	Muted
	Orange
	Amber
	Salmon
	Lavender
	Blue
)

var RGB = [][3]int{{9, 12, 20}, {231, 226, 215}, {121, 133, 156}, {239, 157, 84}, {244, 193, 113}, {215, 137, 144}, {168, 155, 211}, {118, 175, 209}}

// ANSI equivalents deliberately preserve rail contrast in reduced color terminals.
var ANSI = []int{0, 7, 8, 3, 11, 1, 13, 6}

type Cell struct {
	Ch     rune
	FG, BG Color
}
type Canvas struct {
	W, H  int
	Cells []Cell
	ASCII bool
}

func NewCanvas(w, h int, ascii bool) *Canvas {
	w = max(1, min(500, w))
	h = max(1, min(200, h))
	c := &Canvas{W: w, H: h, ASCII: ascii, Cells: make([]Cell, w*h)}
	for i := range c.Cells {
		c.Cells[i] = Cell{' ', Text, Background}
	}
	return c
}
func (c *Canvas) Put(x, y int, ch rune, fg, bg Color) {
	if x < 0 || x >= c.W || y < 0 || y >= c.H {
		return
	}
	if c.ASCII && ch > 127 {
		switch ch {
		case '▀', '▄', '█', '▌', '▐', '▛', '▜', '▙', '▟':
			ch = '#'
		case '─', '━':
			ch = '-'
		case '│', '┃':
			ch = '|'
		case '•', '●', '◉':
			ch = 'o'
		case '…':
			ch = '~'
		default:
			ch = '.'
		}
	}
	c.Cells[y*c.W+x] = Cell{ch, fg, bg}
}

// Strings from the OS cannot inject terminal escapes or introduce wide cells.
// Combining marks are skipped; wide runes become a single-cell '?' consistently.
func Safe(s string) string {
	var b strings.Builder
	for _, r := range s {
		if unicode.IsControl(r) || unicode.Is(unicode.Cf, r) || unicode.Is(unicode.Mn, r) || unicode.Is(unicode.Me, r) {
			continue
		}
		if wide(r) || r == unicode.ReplacementChar {
			r = '?'
		}
		b.WriteRune(r)
	}
	return b.String()
}
func wide(r rune) bool {
	return r >= 0x1100 && (r <= 0x115f || r == 0x2329 || r == 0x232a || (r >= 0x2e80 && r <= 0xa4cf) || (r >= 0xac00 && r <= 0xd7a3) || (r >= 0xf900 && r <= 0xfaff) || (r >= 0xfe10 && r <= 0xfe6f) || (r >= 0xff00 && r <= 0xff60) || (r >= 0xffe0 && r <= 0xffe6) || (r >= 0x1f000 && r <= 0x1faff) || (r >= 0x20000 && r <= 0x3ffff))
}
func Truncate(s string, n int) string {
	r := []rune(Safe(s))
	if n <= 0 {
		return ""
	}
	if len(r) > n {
		r = r[:n]
		r[n-1] = '~'
	}
	return string(r)
}
func (c *Canvas) Write(x, y int, s string, color Color) { c.Text(x, y, c.W-x, s, color, Background) }
func (c *Canvas) Text(x, y, n int, s string, fg, bg Color) {
	for i, r := range []rune(Truncate(s, n)) {
		if c.ASCII && r > 127 {
			r = '?'
		}
		c.Put(x+i, y, r, fg, bg)
	}
}
func (c *Canvas) Fill(x, y, w, h int, color Color) {
	for row := max(0, y); row < min(c.H, y+h); row++ {
		for col := max(0, x); col < min(c.W, x+w); col++ {
			c.Put(col, row, ' ', Text, color)
		}
	}
}
func (c *Canvas) Plain() string {
	var b strings.Builder
	for y := 0; y < c.H; y++ {
		for x := 0; x < c.W; x++ {
			b.WriteRune(c.Cells[y*c.W+x].Ch)
		}
		b.WriteByte('\n')
	}
	return b.String()
}
func hex(color Color) string { v := RGB[color]; return fmt.Sprintf("#%02x%02x%02x", v[0], v[1], v[2]) }

// SVG captures preserve the actual cells and colors; no simulated telemetry.
func (c *Canvas) SVG(w io.Writer) error {
	var b strings.Builder
	fmt.Fprintf(&b, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"%d\" height=\"%d\" viewBox=\"0 0 %d %d\">\n", c.W*10, c.H*20, c.W*10, c.H*20)
	fmt.Fprintf(&b, "<rect width=\"100%%\" height=\"100%%\" fill=\"%s\"/>\n", hex(Background))
	// Background runs use exact cell boundaries, like terminal rendering.
	for y := 0; y < c.H; y++ {
		for x := 0; x < c.W; {
			cell := c.Cells[y*c.W+x]
			end := x + 1
			for end < c.W && c.Cells[y*c.W+end].BG == cell.BG {
				end++
			}
			if cell.BG != Background {
				fmt.Fprintf(&b, "<rect x=\"%d\" y=\"%d\" width=\"%d\" height=\"20\" fill=\"%s\"/>\n", x*10, y*20, (end-x)*10, hex(cell.BG))
			}
			x = end
		}
	}
	b.WriteString("<g font-family=\"DejaVu Sans Mono,monospace\" font-size=\"16\" xml:space=\"preserve\">\n")
	for y := 0; y < c.H; y++ {
		for x := 0; x < c.W; x++ {
			cell := c.Cells[y*c.W+x]
			if cell.Ch == ' ' {
				continue
			}
			fmt.Fprintf(&b, "<text x=\"%d\" y=\"%d\" fill=\"%s\">%s</text>\n", x*10, y*20+16, hex(cell.FG), html.EscapeString(string(cell.Ch)))
		}
	}
	b.WriteString("</g></svg>\n")
	_, err := io.WriteString(w, b.String())
	return err
}
