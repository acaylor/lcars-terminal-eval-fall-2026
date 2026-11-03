package render

// LCARS shape primitives. Every shape degrades: Unicode uses half/quarter
// blocks and box-drawing; ASCII uses '#', '=' and ' '.

// Elbow draws a quarter-round corner (thicker than a box-drawing curve).
// orient names the corner: "tl" = top-left (rail comes from right+botside).
func Elbow(b *Buffer, x, y int, orient string, col RGB, th Theme) {
	set := func(dx, dy int, ch rune, fg, bg RGB) {
		b.Set(x+dx, y+dy, Cell{Ch: ch, Fg: fg, Bg: bg})
	}
	if th.ASCII {
		set(0, 0, '#', col, th.Bg)
		return
	}
	switch orient {
	case "tl": // ╭
		set(0, 0, '╭', col, th.Bg)
	case "tr": // ╮
		set(0, 0, '╮', col, th.Bg)
	case "bl": // ╰
		set(0, 0, '╰', col, th.Bg)
	case "br": // ╯
		set(0, 0, '╯', col, th.Bg)
	}
}

// RailH is a thick horizontal bar of height hh (1-3 rows).
func RailH(b *Buffer, x, y, w, hh int, col RGB, th Theme) {
	ch := '█'
	if th.ASCII {
		ch = '#'
	}
	for dy := 0; dy < hh; dy++ {
		for dx := 0; dx < w; dx++ {
			b.Set(x+dx, y+dy, Cell{Ch: ch, Fg: col, Bg: col})
		}
	}
}

// RailV is a thick vertical bar of width ww (1-3 cols).
func RailV(b *Buffer, x, y, ww, h int, col RGB, th Theme) {
	ch := '█'
	if th.ASCII {
		ch = '#'
	}
	for dy := 0; dy < h; dy++ {
		for dx := 0; dx < ww; dx++ {
			b.Set(x+dx, y+dy, Cell{Ch: ch, Fg: col, Bg: col})
		}
	}
}

// PillH draws a horizontal pill/terminator: a rounded cap + body.
func PillH(b *Buffer, x, y, w int, col RGB, th Theme, label string, lblFg RGB) {
	if th.ASCII {
		RailH(b, x, y, w, 1, col, th)
	} else {
		// rounded ends via half circles approximated with blocks
		b.Set(x, y, Cell{Ch: '▐', Fg: col, Bg: th.Bg})
		b.Set(x+w-1, y, Cell{Ch: '▌', Fg: col, Bg: th.Bg})
		for dx := 1; dx < w-1; dx++ {
			b.Set(x+dx, y, Cell{Ch: '█', Fg: col, Bg: col})
		}
	}
	if label != "" {
		DrawText(b, x+2, y, clip(label, w-3), lblFg, col)
	}
}

// PillV draws a vertical pill.
func PillV(b *Buffer, x, y, h int, col RGB, th Theme) {
	if th.ASCII {
		RailV(b, x, y, 1, h, col, th)
		return
	}
	b.Set(x, y, Cell{Ch: '▄', Fg: col, Bg: th.Bg})
	b.Set(x, y+h-1, Cell{Ch: '▀', Fg: col, Bg: th.Bg})
	for dy := 1; dy < h-1; dy++ {
		b.Set(x, y+dy, Cell{Ch: '█', Fg: col, Bg: col})
	}
}

// RoundedPanel fills [x,x+w) x [y,y+h) and adds elbow corners over it.
// In ASCII mode corners are '#'.
func RoundedPanel(b *Buffer, x, y, w, h int, col RGB, th Theme) {
	b.FillRect(x, y, w, h, Cell{Ch: ' ', Fg: th.Fg, Bg: col})
	if th.ASCII {
		for _, p := range [][2]int{{x, y}, {x + w - 1, y}, {x, y + h - 1}, {x + w - 1, y + h - 1}} {
			b.Set(p[0], p[1], Cell{Ch: '#', Fg: col, Bg: th.Bg})
		}
		return
	}
	b.Set(x, y, Cell{Ch: '╭', Fg: col, Bg: th.Bg})
	b.Set(x+w-1, y, Cell{Ch: '╮', Fg: col, Bg: th.Bg})
	b.Set(x, y+h-1, Cell{Ch: '╰', Fg: col, Bg: th.Bg})
	b.Set(x+w-1, y+h-1, Cell{Ch: '╯', Fg: col, Bg: th.Bg})
}

func clip(s string, w int) string {
	r := []rune(s)
	if len(r) <= w {
		return s
	}
	if w <= 1 {
		return "…"
	}
	return string(r[:w-1]) + "…"
}

// DrawText writes s at (x,y); s must be pre-sanitized by the caller.
func DrawText(b *Buffer, x, y int, s string, fg, bg RGB) {
	for i, r := range []rune(s) {
		b.Set(x+i, y, Cell{Ch: r, Fg: fg, Bg: bg})
	}
}

// DrawTextAttr is DrawText with attributes.
func DrawTextAttr(b *Buffer, x, y int, s string, fg, bg RGB, attr uint8) {
	for i, r := range []rune(s) {
		b.Set(x+i, y, Cell{Ch: r, Fg: fg, Bg: bg, Attr: attr})
	}
}

// EndCap draws a left-side rounded cap glyph for a rail end.
func EndCap(b *Buffer, x, y int, col RGB, th Theme, right bool) {
	if th.ASCII {
		b.Set(x, y, Cell{Ch: '#', Fg: col, Bg: th.Bg})
		return
	}
	if right {
		b.Set(x, y, Cell{Ch: '▐', Fg: col, Bg: th.Bg})
	} else {
		b.Set(x, y, Cell{Ch: '▌', Fg: col, Bg: th.Bg})
	}
}
