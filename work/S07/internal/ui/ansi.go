package ui

import (
	"fmt"
	"strings"
)

type ColorMode string

const (
	TrueColor ColorMode = "truecolor"
	Indexed   ColorMode = "256"
	Basic     ColorMode = "16"
	Mono      ColorMode = "mono"
)

func DetectColor(requested, term, colorterm string, noColor bool) ColorMode {
	if requested != "auto" {
		return ColorMode(requested)
	}
	if noColor {
		return Mono
	}
	if strings.Contains(strings.ToLower(colorterm), "truecolor") || strings.Contains(colorterm, "24bit") || strings.Contains(term, "direct") {
		return TrueColor
	}
	if strings.Contains(term, "256color") {
		return Indexed
	}
	return Basic
}
func rgb(c Color) (int, int, int) { return int(c>>16) & 255, int(c>>8) & 255, int(c) & 255 }
func sgr(c Color, bg bool, mode ColorMode) string {
	base := 38
	if bg {
		base = 48
	}
	r, g, b := rgb(c)
	switch mode {
	case TrueColor:
		return fmt.Sprintf("\x1b[%d;2;%d;%d;%dm", base, r, g, b)
	case Indexed:
		index := 16 + 36*((r*5+127)/255) + 6*((g*5+127)/255) + (b*5+127)/255
		if c == Background {
			index = 232
		}
		return fmt.Sprintf("\x1b[%d;5;%dm", base, index)
	case Mono:
		n := 37
		if c == Background {
			n = 30
		} else if c == Dim {
			n = 90
		}
		if bg {
			n += 10
		}
		return fmt.Sprintf("\x1b[%dm", n)
	default:
		n := 37
		switch c {
		case Background:
			n = 30
		case Dim, Muted:
			n = 90
		case Orange, Amber:
			n = 93
		case Salmon:
			n = 91
		case Lavender:
			n = 95
		case Blue:
			n = 96
		case White:
			n = 97
		}
		if bg {
			n += 10
		}
		return fmt.Sprintf("\x1b[%dm", n)
	}
}

// Diff emits only changed spans. The caller disables autowrap for the last cell.
func Diff(previous, current *Canvas, mode ColorMode) string {
	full := previous == nil || previous.W != current.W || previous.H != current.H
	var b strings.Builder
	var style Cell
	haveStyle := false
	for y := 0; y < current.H; y++ {
		for x := 0; x < current.W; {
			i := y*current.W + x
			if !full && previous.Cells[i] == current.Cells[i] {
				x++
				continue
			}
			fmt.Fprintf(&b, "\x1b[%d;%dH", y+1, x+1)
			for x < current.W {
				i = y*current.W + x
				cell := current.Cells[i]
				if !full && previous.Cells[i] == cell {
					break
				}
				if !haveStyle || style.FG != cell.FG {
					b.WriteString(sgr(cell.FG, false, mode))
				}
				if !haveStyle || style.BG != cell.BG {
					b.WriteString(sgr(cell.BG, true, mode))
				}
				haveStyle = true
				style = cell
				b.WriteRune(cell.Rune)
				x++
			}
		}
	}
	if b.Len() > 0 {
		b.WriteString("\x1b[0m")
	}
	return b.String()
}
