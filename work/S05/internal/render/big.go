package render

// Big-digit font: 3 rows tall, variable width, built from Unicode
// half/quarter blocks. In ASCII mode the shapes are '#', '=' and ' '.

var bigGlyphs = map[rune][3]string{
	'0': {"▄██▄", "█  █", "▀██▀"},
	'1': {" ▄█ ", "  █ ", "  █ "},
	'2': {"▄██▄", " ▄█ ", "█▄▄▄"},
	'3': {"▄██▄", " ▄█ ", "▀██▀"},
	'4': {"█  █", "▀███", "   █"},
	'5': {"████", "█▄▄ ", "▄▄▀█"},
	'6': {"▄██▄", "█▄  ", "▀██▀"},
	'7': {"████", "  █ ", "  █ "},
	'8': {"▄██▄", "▀██▀", "▀██▀"},
	'9': {"▄██▄", " ▀▀█", "▀██▀"},
	':': {" ", "▀", "▀"},
	'.': {" ", " ", "▀"},
	' ': {"  ", "  ", "  "},
	'A': {"▄▀▄", "█▀█", "▀ ▀"},
	'P': {"█▀▄", "█▄ ", "▀  "},
	'M': {"█▄ ▄█", "█ ▀ █", "▀   ▀"},
}

var bigGlyphsASCII = map[rune][3]string{
	'0': {" ## ", "#  #", " ## "},
	'1': {"  # ", "  # ", "  # "},
	'2': {" ## ", "  # ", "#   "}, // tweaked below via map override
	':': {" ", "#", "#"},
	'.': {" ", " ", "#"},
	' ': {"  ", "  ", "  "},
	'A': {" # ", "# #", "###"},
	'P': {"## ", "# #", "#  "},
	'M': {"#   #", "## ##", "#   #"},
}

func init() {
	// Fill digits 0-9 for ASCII from block shapes replaced by '#'.
	for d := '0'; d <= '9'; d++ {
		g := bigGlyphs[d]
		var a [3]string
		for i, row := range g {
			a[i] = asciiBlock(row)
		}
		bigGlyphsASCII[d] = a
	}
	bigGlyphsASCII['2'] = [3]string{" ## ", "  # ", "####"}
	bigGlyphsASCII[':'] = [3]string{" ", "#", "#"}
}

func asciiBlock(s string) string {
	b := []rune(s)
	for i, r := range b {
		switch r {
		case '▄', '█', '▀':
			b[i] = '#'
		}
	}
	return string(b)
}

// DrawBig writes s using the 3-row font at (x,y) with fg on bg. Runes
// without glyphs fall back to narrow text. Returns the width consumed.
func DrawBig(b *Buffer, x, y int, s string, fg, bg RGB, ascii bool) int {
	font := bigGlyphs
	if ascii {
		font = bigGlyphsASCII
	}
	cx := x
	for _, r := range s {
		g, ok := font[r]
		if !ok {
			b.Set(cx, y+1, Cell{Ch: r, Fg: fg, Bg: bg})
			cx++
			continue
		}
		w := runeLen(g[0])
		for dy := 0; dy < 3; dy++ {
			for dx, ch := range []rune(g[dy]) {
				c := Cell{Ch: ' ', Fg: fg, Bg: bg}
				if ch != ' ' {
					c.Ch = ch
					c.Fg = fg
				}
				b.Set(cx+dx, y+dy, c)
			}
		}
		cx += w + 1
	}
	return cx - x - 1
}

func runeLen(s string) int { return len([]rune(s)) }

// Braille trace: encode vals (0..1) as a braille sparkline of width w.
// Each braille cell packs 2x4 dots; we draw one column per value with a
// dot at the quantized height, giving a smooth trace.
func DrawTrace(b *Buffer, x, y, w, h int, vals []float64, fg, bg RGB, ascii bool) {
	if w <= 0 || h <= 0 || len(vals) == 0 {
		return
	}
	if ascii {
		chars := []rune(" .:-=+*#%@")
		for i := 0; i < w; i++ {
			v := vals[i*len(vals)/w]
			lvl := int(v * float64(len(chars)-1))
			b.Set(x+i, y+h-1, Cell{Ch: chars[lvl], Fg: fg, Bg: bg})
		}
		return
	}
	// Map values to a grid of dot rows: 4 per cell row.
	rows := h * 4
	for i := 0; i < w; i++ {
		// two sub-columns per braille cell
		for sub := 0; sub < 2; sub++ {
			idx := (i*2 + sub) * len(vals) / (w * 2)
			if idx >= len(vals) {
				idx = len(vals) - 1
			}
			dotRow := rows - 1 - int(vals[idx]*float64(rows-1))
			cellY := y + dotRow/4
			bit := brailleBit(sub, dotRow%4)
			cur := b.Get(x+i, cellY)
			base := cur.Ch
			if base < 0x2800 || base > 0x28FF {
				base = 0x2800
			}
			b.Set(x+i, cellY, Cell{Ch: base | rune(1<<uint(bit)), Fg: fg, Bg: bg})
		}
	}
}

// brailleBit returns the dot bit for column (0 left, 1 right) and row 0..3.
func brailleBit(col, row int) int {
	// Unicode braille dot numbering:
	// col0: dots 1,2,3,7 (rows 0..3) = bits 0,1,2,6
	// col1: dots 4,5,6,8 (rows 0..3) = bits 3,4,5,7
	if col == 0 {
		return []int{0, 1, 2, 6}[row]
	}
	return []int{3, 4, 5, 7}[row]
}
