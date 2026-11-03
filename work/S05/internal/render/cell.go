package render

import "strconv"

func itoa(i int) string { return strconv.Itoa(i) }

type Cell struct {
	Ch rune
	Fg RGB
	Bg RGB
	// Attribute bits packed as a small int to keep Cell comparable with ==.
	Attr uint8
}

const (
	AttrBold uint8 = 1 << iota
	AttrDim
	AttrReverse
	AttrUnderline
)

// Buffer is a w*h grid of cells plus a cursor position (unused by views).
type Buffer struct {
	W, H  int
	Cells []Cell
}

func NewBuffer(w, h int, fill Cell) *Buffer {
	if w < 0 {
		w = 0
	}
	if h < 0 {
		h = 0
	}
	b := &Buffer{W: w, H: h, Cells: make([]Cell, w*h)}
	for i := range b.Cells {
		b.Cells[i] = fill
	}
	return b
}

func (b *Buffer) idx(x, y int) int { return y*b.W + x }

func (b *Buffer) In(x, y int) bool { return x >= 0 && y >= 0 && x < b.W && y < b.H }

func (b *Buffer) Get(x, y int) Cell {
	if !b.In(x, y) {
		return Cell{Ch: ' '}
	}
	return b.Cells[b.idx(x, y)]
}

func (b *Buffer) Set(x, y int, c Cell) {
	if b.In(x, y) {
		b.Cells[b.idx(x, y)] = c
	}
}

func (b *Buffer) FillRect(x, y, w, h int, c Cell) {
	for yy := y; yy < y+h; yy++ {
		for xx := x; xx < x+w; xx++ {
			b.Set(xx, yy, c)
		}
	}
}

// HLine fills [x, x+w) on row y with the cell's rune.
func (b *Buffer) HLine(x, y, w int, c Cell) { b.FillRect(x, y, w, 1, c) }
func (b *Buffer) VLine(x, y, h int, c Cell) { b.FillRect(x, y, 1, h, c) }

// RuneLen returns the display cell count of s (every scalar = 1 col; the
// views only lay out narrow glyphs and degrade wide ones upstream).
func RuneLen(s string) int { return len([]rune(s)) }

// String returns the rune grid only — handy for golden tests.
func (b *Buffer) String() string {
	out := make([]rune, 0, (b.W+1)*b.H)
	for y := 0; y < b.H; y++ {
		for x := 0; x < b.W; x++ {
			r := b.Cells[b.idx(x, y)].Ch
			if r == 0 {
				r = ' '
			}
			out = append(out, r)
		}
		out = append(out, '\n')
	}
	return string(out)
}
