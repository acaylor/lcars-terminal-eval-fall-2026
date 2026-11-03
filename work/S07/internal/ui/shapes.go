package ui

// Quadrant rasterization gives the elbows sub-cell edges without font-dependent
// private-use glyphs. Vertical coordinates are doubled to match terminal aspect.
func (c *Canvas) Elbow(x, y, w, h, spine, rail int, color Color, flip bool) {
	outer := float64(min(spine, 8))
	inner := float64(min(spine/2, 5))
	for row := 0; row < h; row++ {
		for col := 0; col < w; col++ {
			mask := 0
			for sy := 0; sy < 2; sy++ {
				for sx := 0; sx < 2; sx++ {
					px := float64(col) + (float64(sx)+0.5)/2
					py := float64(row)*2 + float64(sy) + 0.5
					if flip {
						py = float64(h)*2 - py
					}
					inside := px < float64(spine) || py < float64(rail*2)
					if px < float64(spine)+inner && py < float64(rail*2)+inner {
						dx, dy := px-float64(spine)-inner, py-float64(rail*2)-inner
						if dx < 0 && dy < 0 && dx*dx+dy*dy > inner*inner {
							inside = true
						}
					}
					if px < outer && py < outer {
						dx, dy := px-outer, py-outer
						if dx*dx+dy*dy > outer*outer {
							inside = false
						}
					}
					if inside {
						mask |= 1 << (sy*2 + sx)
					}
				}
			}
			glyphs := [16]rune{' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█'}
			if mask == 15 {
				c.Fill(x+col, y+row, 1, 1, color)
			} else if mask != 0 {
				c.Put(x+col, y+row, glyphs[mask], color, Background)
			}
		}
	}
}
