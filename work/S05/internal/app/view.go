// Package app wires metrics, rendering and input into the LCARS display.
//
// Layout thesis (why this is not a dashboard):
//   - A thick orange rail hugs the left edge full-height, elbowed into a
//     deep top header band: a dominant asymmetric L (the left rail is 6
//     cols, the right side is open).
//   - A giant clock (3-row half-block glyphs) anchors the top third.
//   - Below, a sweeping amber band slashes diagonally (staircase elbows)
//     carrying CPU/MEM/DISK gauges and a braille NET trace. The content
//     column is staggered inside the sweep, not boxed.
//   - The lower-right is intentional void with only a faint dot grid.
//   - The left rail is segmented into labelled pills (SYS NET MEM DSK …).
package app

import (
	"fmt"
	"strings"
	"time"

	"lcars/internal/metrics"
	"lcars/internal/render"
)

type View int

const (
	ViewPrimary View = iota
	ViewDiag
)

type State struct {
	Snap      metrics.Snapshot
	Rates     metrics.Rates
	HasRates  bool
	View      View
	Help      bool
	Paused    bool
	Tick      int
	CPURing   []float64 // 0..1 history, newest last, cap 120
	RxRing    []float64
	CPUScale  float64 // auto-scale max for braille traces
	RxScale   float64
	TermW     int
	TermH     int
	Mode      render.ColorMode
	Ascii     bool
	StartedAt time.Time
}

func (s *State) theme() render.Theme {
	return render.DefaultTheme().ForMode(s.Mode, s.Ascii)
}

const ringCap = 120

func pushRing(r []float64, v float64) []float64 {
	r = append(r, v)
	if len(r) > ringCap {
		r = append([]float64{}, r[len(r)-ringCap:]...)
	}
	return r
}

func (s *State) pushSample() {
	cpu := 0.0
	if s.Rates.CPUPctOK {
		cpu = s.Rates.CPUPct / 100
	}
	s.CPURing = pushRing(s.CPURing, clamp01(cpu))
	rx := 0.0
	if s.Rates.NetOK {
		// normalize by a rolling scale (>= 1MB/s)
		if s.RxScale < 1 {
			s.RxScale = 1 << 20
		}
		rx = clamp01(float64(s.Rates.NetRx) / s.RxScale)
		rx = clamp01(rx)
		m := maxf(float64(s.Rates.NetRx), float64(s.Rates.NetTx))
		// scale decays slowly toward current peak
		s.RxScale = s.RxScale*0.9 + m*0.11
		if s.RxScale < 1024 {
			s.RxScale = 1024
		}
	}
	s.RxRing = pushRing(s.RxRing, rx)
	if s.Rates.CPUPctOK {
		p := s.Rates.CPUPct / 100
		if p > s.CPUScale {
			s.CPUScale = p
		} else {
			s.CPUScale = p*0.05 + s.CPUScale*0.95
		}
		if s.CPUScale < 0.1 {
			s.CPUScale = 0.1
		}
	}
}

func clamp01(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}
func maxf(a, b float64) float64 {
	if a > b {
		return a
	}
	return b
}

// Build constructs the frame for the given state at W x H.
func Build(s *State) *render.Buffer {
	th := s.theme()
	buf := render.NewBuffer(s.TermW, s.TermH, render.Cell{Ch: ' ', Fg: th.Fg, Bg: th.Bg})
	if s.TermW < 20 || s.TermH < 8 {
		render.DrawText(buf, 0, 0, "TERMINAL TOO SMALL", th.Warn, th.Bg)
		return buf
	}
	if s.Help {
		buildHelp(buf, *s, th)
		return buf
	}
	if s.View == ViewDiag {
		buildDiag(buf, *s, th)
		return buf
	}
	if s.TermW < 100 || s.TermH < 28 {
		buildCompact(buf, *s, th)
		return buf
	}
	buildPrimary(buf, *s, th)
	return buf
}

// block is a colour-field glyph cell of given colour.
func blk(col render.RGB, th render.Theme) render.Cell {
	ch := '█'
	if th.ASCII {
		ch = '#'
	}
	return render.Cell{Ch: ch, Fg: col, Bg: col}
}

func buildPrimary(b *render.Buffer, s State, th render.Theme) {
	W, H := b.W, b.H
	now := s.Snap.At
	if now.IsZero() {
		now = time.Now()
	}
	orange, peach, salmon, lilac, blue, cool := th.Rails[0], th.Rails[1], th.Rails[2], th.Rails[3], th.Rails[4], th.Rails[5]

	// -- background void texture (lower-right dot grid) ---------------
	for y := H * 3 / 5; y < H-2; y += 2 {
		for x := W/2 + 4; x < W-2; x += 4 {
			b.Set(x, y, render.Cell{Ch: dotGlyph(s.Ascii), Fg: th.Dim, Bg: th.Bg})
		}
	}

	// -- left rail (full height, thick) --------------------------------
	railW := 7
	if W < 100 {
		railW = 6
	}
	segs := []struct {
		col   render.RGB
		label string
		h     int
	}{
		{orange, "LCARS", 4},
		{peach, "SYS", 6},
		{salmon, "CPU", 7},
		{lilac, "MEM", 6},
		{blue, "DSK", 6},
	}
	y := 0
	for _, sg := range segs {
		for dy := 0; dy < sg.h && y+dy < H; dy++ {
			for dx := 0; dx < railW; dx++ {
				b.Set(dx, y+dy, blk(sg.col, th))
			}
		}
		if sg.label != "" {
			// Rotated label: runes stacked vertically inside the rail,
			// reading bottom-up — survives plain-text buffer dumps,
			// unlike a 1-col-per-rune attempt in a 7-col rail
			startY := y + (sg.h-len([]rune(sg.label)))/2
			for i, r := range []rune(sg.label) {
				b.Set(3, startY+i, render.Cell{Ch: r, Fg: render.Black, Bg: sg.col, Attr: render.AttrBold})
			}
		}
		y += sg.h + 1 // black gap
	}
	// bottom of rail: blinking status block
	blinkCol := cool
	if s.Tick%2 == 0 {
		blinkCol = blue
	}
	for ; y < H; y++ {
		for dx := 0; dx < railW; dx++ {
			b.Set(dx, y, blk(blinkCol, th))
		}
	}

	// -- top header band -----------------------------------------------
	hx := railW + 1
	hw := 46
	if W >= 130 {
		hw = 54
	}
	if hx+hw > W-2 {
		hw = W - hx - 2
	}
	render.RailH(b, hx, 0, hw, 2, orange, th)
	render.DrawTextAttr(b, hx+2, 0, "LCARS // ENGINEERING", render.Black, orange, render.AttrBold)
	render.Elbow(b, hx+hw, 0, "tr", orange, th)
	render.Elbow(b, hx+hw, 1, "tr", orange, th)
	// short salmon stub to the right of the header
	sx := hx + hw + 2
	if sx+14 < W {
		render.PillH(b, sx, 0, 14, salmon, th, "", render.Black)
		render.PillH(b, sx, 1, 10, peach, th, "", render.Black)
	}
	// status badge (top right)
	badge := "ONLINE"
	badgeCol := cool
	if s.Paused {
		badge = "PAUSED"
		badgeCol = th.Warn
	}
	bx := W - render.RuneLen(badge) - 3
	render.DrawTextAttr(b, bx, 0, badge, badgeCol, th.Bg, render.AttrBold)

	// -- giant clock (focal element) -------------------------------------
	cx := hx + 2
	cy := 4
	dateStr := now.Format("2006-01-02") + "  " + strings.ToUpper(now.Format("Mon"))
	timeStr := now.Format("15:04:05")
	render.DrawBig(b, cx, cy, timeStr, orange, th.Bg, s.Ascii)
	// date + host line under the clock
	sub := dateStr + "  //  " + hostnameShort(s)
	if !s.Ascii {
		sub = dateStr + "   ⬢  " + hostnameShort(s) // decorative separator
	}
	render.DrawText(b, cx, cy+4, clipStr(sub, W-cx-2), th.Dim, th.Bg)
	render.DrawTextAttr(b, cx+11, cy+5, timeStr, peach, th.Bg, render.AttrBold)
	// AM/PM chip
	ampm := now.Format("PM")
	if now.Hour() < 12 {
		ampm = "AM"
	}
	render.PillH(b, cx, cy+5, 9, peach, th, " "+ampm+" ", render.Black)

	// -- load focal readout (right of clock, big) ------------------------
	lx := cx + 34
	if lx+26 < W {
		loadVals := [3]string{"--", "--", "--"}
		if s.Rates.CPUPctOK || s.Snap.LoadOK {
			loadVals = [3]string{f2(s.Snap.Load[0]), f2(s.Snap.Load[1]), f2(s.Snap.Load[2])}
		}
		render.DrawTextAttr(b, lx, cy-1, "LOAD", lilac, th.Bg, render.AttrBold)
		render.DrawBig(b, lx, cy, loadVals[0], lilac, th.Bg, s.Ascii)
		render.DrawText(b, lx, cy+4, "1M  "+loadVals[0]+"   5M  "+loadVals[1]+"   15M  "+loadVals[2], th.Fg, th.Bg)
	}

	// -- amber sweep band (diagonal) -------------------------------------
	// Rows bandY..bandY+4; left edge staircases right as y grows.
	bandY := 13
	bandH := 4
	if H < 30 {
		bandY = 12
		bandH = 3
	}
	bandLeft := func(y int) int { return hx + (y-bandY)*3 }
	for yy := bandY; yy < bandY+bandH && yy < H-3; yy++ {
		l := bandLeft(yy)
		for x := l; x < W-10; x++ {
			b.Set(x, yy, render.Cell{Ch: ' ', Fg: th.Fg, Bg: salmon})
		}
		if !s.Ascii {
			b.Set(l-1, yy, render.Cell{Ch: '▐', Fg: salmon, Bg: th.Bg})
			b.Set(W-10, yy, render.Cell{Ch: '▌', Fg: salmon, Bg: th.Bg})
		} else {
			b.Set(l-1, yy, render.Cell{Ch: '#', Fg: salmon, Bg: th.Bg})
			b.Set(W-10, yy, render.Cell{Ch: '#', Fg: salmon, Bg: th.Bg})
		}
	}
	// gauges on the band
	gx := hx + 2
	gy := bandY
	cpuTxt := "CPU  N/A"
	if s.HasRates && s.Rates.CPUPctOK {
		cpuTxt = fmt.Sprintf("CPU %5.1f%%", s.Rates.CPUPct)
	}
	render.DrawTextAttr(b, gx, gy, cpuTxt, render.Black, salmon, render.AttrBold)
	// mini bar from CPU%
	barW := 16
	pct := 0.0
	if s.Rates.CPUPctOK {
		pct = s.Rates.CPUPct
	}
	render.DrawText(b, gx, gy+1, barStr(pct, barW, s.Ascii), render.Black, salmon)
	memTxt := "MEM  N/A"
	if s.Snap.MemOK {
		memTxt = fmt.Sprintf("MEM %s/%s", metrics.BytesString(s.Snap.Mem.Used), metrics.BytesString(s.Snap.Mem.Total))
	} else if s.Snap.Mem.Total > 0 {
		memTxt = "MEM " + metrics.BytesString(s.Snap.Mem.Total) + " TOT"
	}
	render.DrawTextAttr(b, gx+28, gy, memTxt, render.Black, salmon, render.AttrBold)
	dskTxt := "DSK  N/A"
	if s.Snap.DiskOK {
		dskTxt = fmt.Sprintf("DSK %s/%s", metrics.BytesString(s.Snap.Disk.Used), metrics.BytesString(s.Snap.Disk.Total))
	}
	render.DrawTextAttr(b, gx+28, gy+1, dskTxt, render.Black, salmon, render.AttrBold)

	// -- telemetry column (right, open; no boxes) --------------------------
	tx := W - 34
	ty := 10
	telHdr := "> TELEMETRY"
	if !s.Ascii {
		telHdr = "\u25e2 TELEMETRY"
	}
	render.DrawTextAttr(b, tx, ty, telHdr, cool, th.Bg, render.AttrBold)
	lines := []struct {
		k, v string
		c    render.RGB
	}{
		{"HOST", hostnameShort(s), th.Fg},
		{"UPTIME", uptimeOrNA(s), th.Fg},
		{"CPU", cpuVal(s), th.Fg},
		{"RX", rateOrNA(s.Rates.NetRx, s), cool},
		{"TX", rateOrNA(s.Rates.NetTx, s), blue},
	}
	for i, ln := range lines {
		b.Set(tx, ty+2+i, render.Cell{Ch: '▎', Fg: ln.c, Bg: th.Bg})
		if s.Ascii {
			b.Set(tx, ty+2+i, render.Cell{Ch: '|', Fg: ln.c, Bg: th.Bg})
		}
		render.DrawText(b, tx+2, ty+2+i, padTo(ln.k, 6), th.Dim, th.Bg)
		render.DrawText(b, tx+9, ty+2+i, ln.v, ln.c, th.Bg)
	}

	// -- NET braille trace -------------------------------------------------
	ny := H - 9
	if ny < bandY+bandH+2 {
		ny = bandY + bandH + 2
	}
	render.DrawText(b, hx, ny, "NET", th.Dim, th.Bg)
	traceW := W - hx - 14
	cpuScaled := make([]float64, len(s.CPURing))
	for i, v := range s.CPURing {
		cpuScaled[i] = clamp01(v / maxf(0.05, s.CPUScale))
	}
	render.DrawTrace(b, hx, ny+1, traceW, 3, cpuScaled, orange, th.Bg, s.Ascii)

	// -- bottom rails -------------------------------------------------------
	by := H - 3
	render.RailH(b, hx, by, 22, 2, blue, th)
	render.DrawTextAttr(b, hx+2, by, "ENG-042", render.Black, blue, render.AttrBold)
	pillW := 10
	for i, c := range []render.RGB{lilac, peach, orange} {
		px := hx + 26 + i*(pillW+2)
		if px+pillW < W {
			render.PillH(b, px, by, pillW, c, th, "", render.Black)
		}
	}
	keys := "TAB VIEW | P DIAG | SPACE PAUSE | ? HELP | Q QUIT"
	render.DrawText(b, W-render.RuneLen(keys)-2, H-1, keys, th.Dim, th.Bg)
	// bottom-right numeric cluster (decoration)
	render.DrawText(b, W-14, H-2, "NCC-1701-D", th.Dim, th.Bg)
}

func buildCompact(b *render.Buffer, s State, th render.Theme) {
	W, H := b.W, b.H
	now := s.Snap.At
	if now.IsZero() {
		now = time.Now()
	}
	orange, lilac, blue := th.Rails[0], th.Rails[3], th.Rails[5]
	// left stub rail
	render.RailV(b, 0, 0, 2, H, orange, th)
	render.RailH(b, 3, 0, W-3, 1, lilac, th)
	render.DrawTextAttr(b, 4, 0, "LCARS", render.Black, lilac, render.AttrBold)
	// flat time (no big font in compact)
	render.DrawTextAttr(b, 4, 2, now.Format("15:04:05"), orange, th.Bg, render.AttrBold)
	render.DrawText(b, 14, 2, now.Format("2006-01-02"), th.Fg, th.Bg)
	render.DrawText(b, 4, 3, hostnameShort(s), th.Dim, th.Bg)
	row := 5
	for _, ln := range [][2]string{
		{"CPU", cpuVal(s)}, {"LOAD", loadLine(s)},
		{"MEM", memLine(s)}, {"DSK", dskLine(s)},
		{"NET", netLine(s)}, {"UP", uptimeOrNA(s)},
	} {
		render.DrawText(b, 4, row, padTo(ln[0], 5), th.Dim, th.Bg)
		render.DrawText(b, 10, row, ln[1], th.Fg, th.Bg)
		row++
	}
	traceW := W - 6
	s2 := make([]float64, len(s.CPURing))
	for i, v := range s.CPURing {
		s2[i] = clamp01(v / maxf(0.05, s.CPUScale))
	}
	render.DrawTrace(b, 4, H-3, traceW, 1, s2, blue, th.Bg, s.Ascii)
	render.DrawText(b, 4, H-1, "P DIAG ? HELP Q QUIT", th.Dim, th.Bg)
	if !s.Ascii {
		render.DrawText(b, 4, H-1, "P DIAG ? HELP Q QUIT", th.Dim, th.Bg)
	}
}

func buildDiag(b *render.Buffer, s State, th render.Theme) {
	W, H := b.W, b.H
	lilac, orange, cool, blue := th.Rails[3], th.Rails[0], th.Rails[5], th.Rails[4]
	// header band
	render.RailH(b, 0, 0, W-14, 2, lilac, th)
	render.DrawTextAttr(b, 2, 0, "DIAGNOSTICS", render.Black, lilac, render.AttrBold)
	render.DrawText(b, 2, 1, s.Snap.OS+" / "+s.Snap.Arch, render.Black, lilac)
	render.PillH(b, W-14, 0, 12, orange, th, " P CLOSE", render.Black)
	// stub rail
	render.RailV(b, 0, 3, 2, H-4, orange, th)
	render.DrawTextAttr(b, 4, 3, "PER-CORE", th.Dim, th.Bg, render.AttrBold)
	y := 5
	if len(s.Rates.Cores) == 0 {
		render.DrawText(b, 4, y, "N/A", th.Fg, th.Bg)
		y += 2
	} else {
		for _, c := range s.Rates.Cores {
			// CPU% numbers only fill from 3% (idle jitter would light all).
			bar := barStr(c.Pct, 20, s.Ascii)
			render.DrawText(b, 4, y, padTo(c.Label, 4), th.Dim, th.Bg)
			render.DrawText(b, 9, y, bar, cool, th.Bg)
			render.DrawText(b, 31, y, fmt.Sprintf("%5.1f%%", c.Pct), th.Fg, th.Bg)
			y++
			if y > H-14 {
				render.DrawText(b, 4, y, "+", th.Dim, th.Bg)
				break
			}
		}
		y++
	}
	render.DrawTextAttr(b, 4, y, "SWAP", th.Dim, th.Bg, render.AttrBold)
	render.DrawText(b, 11, y, swapLine(s), th.Fg, th.Bg)
	y++
	render.DrawTextAttr(b, 4, y, "PROCS", th.Dim, th.Bg, render.AttrBold)
	render.DrawText(b, 11, y, itoiA(s.Rates.ProcCount), th.Fg, th.Bg)
	y += 2
	// process table (the only true table in the app)
	render.DrawTextAttr(b, 4, y, "PID      NAME                     CPU%     MEM", blue, th.Bg, render.AttrBold)
	y++
	if !s.Rates.ProcsOK || len(s.Rates.Procs) == 0 {
		render.DrawText(b, 4, y, "N/A", th.Fg, th.Bg)
	} else {
		for _, p := range s.Rates.Procs {
			render.DrawText(b, 4, y, fmt.Sprintf("%-8d %-24s %6.1f %8s", p.PID, clipStr(p.Name, 24), p.CPU, metrics.BytesString(p.Mem)), th.Fg, th.Bg)
			y++
		}
	}
	render.DrawText(b, 4, H-1, "TAB / P RETURN    Q QUIT", th.Dim, th.Bg)
}

func buildHelp(b *render.Buffer, s State, th render.Theme) {
	orange, lilac := th.Rails[0], th.Rails[3]
	render.RailH(b, 0, 0, b.W-8, 2, orange, th)
	render.DrawTextAttr(b, 2, 0, "HELP", render.Black, orange, render.AttrBold)
	render.PillH(b, b.W-6, 0, 5, lilac, th, "", render.Black)
	lines := []string{"", "  q / Esc   quit", "  ?         toggle help", "  Space     pause / resume", "  p         diagnostics", "  Tab       cycle views", "", "  Press ? or Esc to close."}
	for i, l := range lines {
		render.DrawText(b, 2, 3+i, l, th.Fg, th.Bg)
	}
}

// --- helpers ------------------------------------------------------------

func dotGlyph(ascii bool) rune {
	// Always '.' : safe in both modes (reads as starfield stipple).
	_ = ascii
	return '.'
}

func hostnameShort(s State) string {
	h := s.Snap.Hostname
	if h == "" {
		return "UNKNOWN"
	}
	if i := indexByte(h, '.'); i > 0 {
		h = h[:i]
	}
	return h
}

func indexByte(s string, c byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			return i
		}
	}
	return -1
}

func clipStr(s string, w int) string {
	r := []rune(s)
	if len(r) <= w {
		return s
	}
	if w <= 1 {
		return "…"
	}
	return string(r[:w-1]) + "…"
}

func padTo(s string, w int) string {
	for len(s) < w {
		s += " "
	}
	return s
}

func f2(f float64) string { return fmt.Sprintf("%.2f", f) }

func itoiA(i int) string { return fmt.Sprintf("%d", i) }

func cpuVal(s State) string {
	if s.HasRates && s.Rates.CPUPctOK {
		return fmt.Sprintf("%.1f%%", s.Rates.CPUPct)
	}
	return "N/A"
}

func loadLine(s State) string {
	if s.Snap.LoadOK {
		return fmt.Sprintf("%.2f %.2f %.2f", s.Snap.Load[0], s.Snap.Load[1], s.Snap.Load[2])
	}
	return "N/A"
}

func uptimeOrNA(s State) string {
	if s.Snap.Uptime > 0 {
		return metrics.UptimeString(s.Snap.Uptime)
	}
	return "N/A"
}

func memLine(s State) string {
	if s.Snap.MemOK {
		return fmt.Sprintf("%s / %s", metrics.BytesString(s.Snap.Mem.Used), metrics.BytesString(s.Snap.Mem.Total))
	}
	if s.Snap.Mem.Total > 0 {
		return metrics.BytesString(s.Snap.Mem.Total) + " total"
	}
	return "N/A"
}

func dskLine(s State) string {
	if s.Snap.DiskOK {
		return fmt.Sprintf("%s / %s", metrics.BytesString(s.Snap.Disk.Used), metrics.BytesString(s.Snap.Disk.Total))
	}
	return "N/A"
}

func netLine(s State) string {
	if s.HasRates && s.Rates.NetOK {
		return "DN " + metrics.RateString(s.Rates.NetRx) + "  UP " + metrics.RateString(s.Rates.NetTx)
	}
	return "N/A"
}

func swapLine(s State) string {
	if s.Snap.MemOK && s.Snap.Mem.SwapAvailable && s.Snap.Mem.SwapTotal > 0 {
		return fmt.Sprintf("%s / %s", metrics.BytesString(s.Snap.Mem.SwapTotal-s.Snap.Mem.SwapFree), metrics.BytesString(s.Snap.Mem.SwapTotal))
	}
	if s.Snap.MemOK && s.Snap.Mem.SwapAvailable {
		return "none configured"
	}
	return "N/A"
}

func rateOrNA(v uint64, s State) string {
	if s.HasRates && s.Rates.NetOK {
		return metrics.RateString(v)
	}
	return "N/A"
}

func barStr(pct float64, w int, ascii bool) string {
	if w <= 0 {
		return ""
	}
	fill := int(pct / 100 * float64(w))
	if fill > w {
		fill = w
	}
	if ascii {
		return strings.Repeat("#", fill) + strings.Repeat("-", w-fill)
	}
	full := fill
	s := strings.Repeat("█", full)
	if full < w {
		s += strings.Repeat("░", w-full)
	}
	return s
}
