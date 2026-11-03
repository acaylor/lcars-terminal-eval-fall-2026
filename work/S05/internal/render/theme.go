package render

import "image/color"

type ColorMode int

const (
	ModeAuto ColorMode = iota
	ModeTrue
	Mode256
	ModeMono
)

type RGB struct{ R, G, B uint8 }

func (c RGB) ANSI(fg bool) string {
	base := 48
	if fg {
		base = 38
	}
	return "\x1b[" + itoa(base) + ";2;" + itoa(int(c.R)) + ";" + itoa(int(c.G)) + ";" + itoa(int(c.B)) + "m"
}

func (c RGB) ANSI256(fg bool) string {
	base := 48
	if fg {
		base = 38
	}
	return "\x1b[" + itoa(base) + ";5;" + itoa(rgbTo256(c)) + "m"
}

func rgbTo256(c RGB) int {
	// xterm 6x6x6 cube + grayscale ramp choice by luminance error.
	to6 := func(v uint8) int {
		if v < 48 {
			return 0
		}
		if v < 115 {
			return 1
		}
		return int(v-35) / 40
	}
	r, g, b := to6(c.R), to6(c.G), to6(c.B)
	cube := 16 + 36*r + 6*g + b
	cv := [...]uint8{0, 95, 135, 175, 215, 255}
	cubeCol := RGB{cv[r], cv[g], cv[b]}
	// grayscale candidate
	avg := (int(c.R) + int(c.G) + int(c.B)) / 3
	gi := (avg - 3) / 10
	if gi < 0 {
		gi = 0
	}
	if gi > 23 {
		gi = 23
	}
	gray := 232 + gi
	gv := uint8(8 + 10*gi)
	grayCol := RGB{gv, gv, gv}
	if dist(c, cubeCol) <= dist(c, grayCol) {
		return cube
	}
	return gray
}

func dist(a, b RGB) int {
	dr := int(a.R) - int(b.R)
	dg := int(a.G) - int(b.G)
	db := int(a.B) - int(b.B)
	return dr*dr + dg*dg + db*db
}

var _ color.Color = RGB{}

func (c RGB) RGBA() (uint32, uint32, uint32, uint32) {
	return uint32(c.R) * 0x101, uint32(c.G) * 0x101, uint32(c.B) * 0x101, 0xffff
}

var (
	Black        = RGB{0, 0, 0}
	White        = RGB{255, 255, 255}
	Orange       = RGB{255, 153, 0}   // #FF9900
	Peach        = RGB{255, 204, 153} // #FFCC99
	Salmon       = RGB{255, 153, 102} // #FF9966 / #CC6666 family
	Red          = RGB{204, 68, 68}
	Butterscotch = RGB{255, 204, 102} // #FFCC66
	Lilac        = RGB{204, 153, 255} // #CC99FF
	Lav          = RGB{153, 153, 204} // #9999CC
	Blue         = RGB{153, 153, 255} // #9999FF
	CoolBlue     = RGB{153, 204, 255} // #99CCFF
	DarkBg       = RGB{10, 10, 16}
	PanelBg      = RGB{20, 20, 28}
)

type Theme struct {
	Bg, Fg RGB
	Rails  []RGB // orange, peach, salmon, lilac, blue, cool
	Accent RGB
	Dim    RGB
	Warn   RGB
	Crit   RGB
	Mode   ColorMode // resolved mode
	ASCII  bool
}

func DefaultTheme() Theme {
	return Theme{
		Bg: DarkBg, Fg: RGB{230, 225, 215}, Accent: Orange,
		Dim: RGB{110, 108, 130}, Warn: Butterscotch, Crit: Red,
		Rails: []RGB{Orange, Peach, Salmon, Lilac, Blue, CoolBlue},
	}
}

// ForMode returns a theme adjusted for the resolved color mode. Mono keeps
// the same hues; Cell encoding collapses them to attributes.
func (t Theme) ForMode(m ColorMode, ascii bool) Theme {
	t.Mode = m
	t.ASCII = ascii
	return t
}
