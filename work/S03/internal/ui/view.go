package ui

import (
	"fmt"
	"math"
	"strings"
	"time"

	"starfleet/internal/metrics"
)

type State struct {
	Diagnostic, Help, Paused, ASCII bool
	Tick                            uint64
	History                         []metrics.Value
}

func Draw(w, h int, s metrics.Snapshot, state State) *Canvas {
	c := NewCanvas(w, h, state.ASCII)
	if c.W < 78 || c.H < 24 {
		tiny(c, s, state)
		return c
	}
	if state.Diagnostic {
		diagnostic(c, s, state)
	} else {
		command(c, s, state)
	}
	if state.Help {
		help(c)
	}
	return c
}
func val(v metrics.Value, format string) string {
	if !v.OK {
		return "--"
	}
	return fmt.Sprintf(format, v.N)
}
func bytes(v metrics.Value) string {
	if !v.OK {
		return "--"
	}
	n := v.N
	for _, unit := range []string{"B", "KiB", "MiB", "GiB", "TiB", "PiB"} {
		if n < 1024 || unit == "PiB" {
			if unit == "B" {
				return fmt.Sprintf("%.0f %s", n, unit)
			}
			return fmt.Sprintf("%.1f %s", n, unit)
		}
		n /= 1024
	}
	return "--"
}
func used(s metrics.Snapshot) metrics.Value {
	if s.MemTotal.OK && s.MemAvailable.OK {
		return metrics.Known(max(0, s.MemTotal.N-s.MemAvailable.N))
	}
	return metrics.Value{}
}
func up(v metrics.Value) string {
	if !v.OK {
		return "--"
	}
	n := int64(v.N)
	return fmt.Sprintf("%dd %02dh %02dm", n/86400, n/3600%24, n/60%60)
}
func percent(a, b metrics.Value) metrics.Value {
	if !a.OK || !b.OK || b.N <= 0 {
		return metrics.Value{}
	}
	return metrics.Known(100 * a.N / b.N)
}
func status(s metrics.Snapshot, st State) (string, Color) {
	if st.Paused {
		return "HOLD", Lavender
	}
	if len(s.Issues) > 0 {
		return "PARTIAL", Salmon
	}
	if !s.CPU.OK {
		return "SAMPLING", Blue
	}
	if s.CPU.N > 90 {
		return "HIGH LOAD", Salmon
	}
	return "ONLINE", Blue
}
func pill(c *Canvas, x, y, w int, color Color, label string) {
	c.Fill(x+1, y, w-2, 2, color)
	c.Put(x, y, '▗', color, Background)
	c.Put(x+w-1, y, '▖', color, Background)
	c.Put(x, y+1, '▝', color, Background)
	c.Put(x+w-1, y+1, '▘', color, Background)
	c.Text(x+2, y, w-4, label, Background, color)
}
func elbow(c *Canvas, rail int) {
	for y := 1; y < 6; y++ {
		for x := 1; x < rail+7; x++ {
			mask := 0
			for sub := 0; sub < 2; sub++ {
				py := 2*y + sub
				outer := x >= 8 || py >= 8 || (x-8)*(x-8)+(py-8)*(py-8) <= 36
				inner := x >= rail && py >= 4 && (x-rail-6)*(x-rail-6)+(py-10)*(py-10) < 36
				if outer && !inner && (x < rail || py < 10) {
					mask |= 1 << sub
				}
			}
			switch mask {
			case 3:
				c.Fill(x, y, 1, 1, Orange)
			case 1:
				c.Put(x, y, '▀', Orange, Background)
			case 2:
				c.Put(x, y, '▄', Orange, Background)
			}
		}
	}
}
func frame(c *Canvas, st State, title string) (int, int) {
	compact := c.W < 105 || c.H < 34
	rail := 16
	if compact {
		rail = 10
	}
	// An elbow anchors the sweeping top rail to the segmented port column.
	c.Fill(rail, 1, c.W-rail-3, 1, Orange)
	elbow(c, rail)
	c.Fill(2, 7, rail-2, 3, Amber)
	c.Fill(2, 11, rail-2, 2, Lavender)
	c.Text(3, 4, rail-4, "01", Background, Orange)
	c.Text(3, 8, rail-4, "02", Background, Amber)
	c.Text(3, 11, rail-4, "03", Background, Lavender)
	c.Fill(2, 15, rail-2, max(1, c.H-20), Salmon)
	c.Text(3, 16, rail-4, "ENG", Background, Salmon)
	if !compact {
		c.Text(3, 18, rail-4, "SYSTEMS", Background, Salmon)
		c.Text(3, 21, rail-4, "07-ALPHA", Background, Salmon)
	}
	c.Fill(2, c.H-4, rail-2, 2, Lavender)
	c.Fill(rail, c.H-3, c.W-rail-3, 1, Lavender)
	c.Put(1, c.H-4, '▐', Lavender, Background)
	c.Put(1, c.H-3, '▝', Lavender, Background)
	c.Text(rail+7, 1, 25, "LCARS / ENGINEERING", Background, Orange)
	c.Text(c.W-28, 1, 23, "SYSTEM MONITOR  //  01", Background, Orange)
	x := rail + 7
	c.Write(x, 3, title, Muted)
	c.Text(x, c.H-3, c.W-x-5, "q / ESC EXIT    ? HELP    SPACE HOLD    p DIAG    TAB VIEW", Background, Lavender)
	return x, rail
}

var digits = map[rune][5]string{
	'0': {"111", "101", "101", "101", "111"}, '1': {"010", "110", "010", "010", "111"},
	'2': {"111", "001", "111", "100", "111"}, '3': {"111", "001", "111", "001", "111"},
	'4': {"101", "101", "111", "001", "001"}, '5': {"111", "100", "111", "001", "111"},
	'6': {"111", "100", "111", "101", "111"}, '7': {"111", "001", "010", "010", "010"},
	'8': {"111", "101", "111", "101", "111"}, '9': {"111", "101", "111", "001", "111"},
	':': {"0", "1", "0", "1", "0"}, '.': {"0", "0", "0", "0", "1"}, '-': {"000", "000", "111", "000", "000"},
}

func big(c *Canvas, x, y int, s string, scale int, color Color) int {
	for _, r := range s {
		rows, ok := digits[r]
		if !ok {
			continue
		}
		width := len(rows[0])
		for row, line := range rows {
			for col, bit := range line {
				if bit == '1' {
					for k := 0; k < scale; k++ {
						ch := '█'
						if row == 0 {
							ch = '▄'
						}
						if row == 4 {
							ch = '▀'
						}
						c.Put(x+col*scale+k, y+row, ch, color, Background)
					}
				}
			}
		}
		x += width*scale + scale
	}
	return x
}
func command(c *Canvas, s metrics.Snapshot, st State) {
	x, rail := frame(c, st, "COMMAND / LOCAL CHRONOMETRY")
	compact := c.W < 105 || c.H < 34
	scale := 2
	if compact {
		scale = 1
	}
	end := big(c, x, 5, s.At.Local().Format("15:04"), scale, Amber)
	c.Write(end+1, 7, s.At.Local().Format("05"), Salmon)
	c.Write(end+1, 9, "SEC", Muted)
	c.Text(x, 11, c.W-x-2, strings.ToUpper(s.At.Local().Format("Mon 02 Jan 2006"))+"  /  "+Truncate(s.Host, 25), Text, Background)
	label, col := status(s, st)
	if !compact {
		pill(c, c.W-27, 5, 23, col, label)
		c.Write(c.W-25, 8, "CPU  "+val(s.CPU, "%5.1f%%"), Text)
		c.Write(c.W-25, 10, "UP   "+up(s.Uptime), Muted)
		// A broad, stepped transverse rail creates a second asymmetry.
		c.Fill(2, 13, rail-2, 1, Amber)
		c.Fill(rail, 13, c.W-rail-12, 1, Amber)
		c.Put(c.W-12, 13, '▌', Amber, Background)
		c.Text(x, 13, 28, "02 / COMPUTE TELEMETRY", Background, Amber)
		c.Write(x, 16, "LOAD / 1 MIN", Muted)
		load := val(s.Load1, "%.2f")
		if len(load) > 6 {
			load = val(s.Load1, "%.0f")
		}
		big(c, x, 18, load, 1, Lavender)
		c.Write(x, 24, "05m "+val(s.Load5, "%.2f")+"  /  15m "+val(s.Load15, "%.2f"), Muted)
		mx := x + 35
		c.Write(mx, 16, "MEMORY / 07", Muted)
		c.Write(mx, 18, bytes(used(s))+"  IN USE", Text)
		c.Write(mx, 20, bytes(s.MemAvailable)+"  AVAILABLE", Blue)
		c.Write(mx, 22, bytes(s.MemTotal)+"  INSTALLED", Muted)
		rx := c.W - 25
		c.Write(rx, 16, "ROOT / 09", Muted)
		ring(c, rx+3, 18, percent(s.DiskUsed, s.DiskTotal), Orange)
		c.Write(rx, 25, bytes(s.DiskUsed)+" / "+bytes(s.DiskTotal), Muted)
		c.Write(x, 27, "CPU TRACE", Muted)
		trace(c, x+12, 27, min(38, c.W-x-41), st.History, Blue)
		c.Write(x, 29, "RX  "+bytes(s.RX)+"/s", Blue)
		c.Write(x+28, 29, "TX  "+bytes(s.TX)+"/s", Salmon)
		c.Write(c.W-25, 29, "LINK / ALL EXCL. LO", Muted)
		c.Write(x, 31, "NETWORK / 03", Muted)
		for i := 0; i < 7; i++ {
			ch := '─'
			if int(st.Tick%7) == i {
				ch = '●'
			}
			c.Put(x+17+i*2, 31, ch, Blue, Background)
		}
	} else {
		pill(c, c.W-25, 5, 22, col, label)
		c.Write(c.W-23, 8, "CPU "+val(s.CPU, "%.1f%%"), Text)
		c.Write(c.W-23, 10, "UP "+up(s.Uptime), Muted)
		c.Fill(rail, 13, c.W-rail-3, 1, Amber)
		c.Text(x, 13, 36, "02 / RESOURCE TELEMETRY", Background, Amber)
		c.Write(x, 15, "LOAD  "+val(s.Load1, "%.2f")+" / "+val(s.Load5, "%.2f")+" / "+val(s.Load15, "%.2f"), Lavender)
		c.Write(x, 17, "MEM   "+bytes(used(s))+" / "+bytes(s.MemTotal), Text)
		c.Write(c.W-29, 17, "FREE  "+bytes(s.MemAvailable), Blue)
		c.Write(x, 19, "RX "+bytes(s.RX)+"/s  TX "+bytes(s.TX)+"/s", Blue)
		c.Write(c.W-29, 19, "ROOT "+val(percent(s.DiskUsed, s.DiskTotal), "%.0f%%")+" "+bytes(s.DiskTotal), Orange)
	}
}
func ring(c *Canvas, x, y int, v metrics.Value, color Color) {
	// Two vertical samples per cell make a curved, open telemetry aperture.
	for row := 0; row < 6; row++ {
		for col := 0; col < 14; col++ {
			mask := 0
			for sub := 0; sub < 2; sub++ {
				dx := (float64(col) - 6.5) / 6.5
				dy := (float64(row*2+sub) - 5.5) / 5.5
				rad := dx*dx + dy*dy
				if rad > .58 && rad < 1.15 {
					mask |= 1 << sub
				}
			}
			if mask > 0 {
				ch := map[int]rune{1: '▀', 2: '▄', 3: '█'}[mask]
				c.Put(x+col, y+row, ch, color, Background)
			}
		}
	}
	c.Text(x+4, y+2, 8, val(v, "%.0f%%"), Text, Background)
	c.Write(x+4, y+3, "USED", Muted)
}
func trace(c *Canvas, x, y, w int, values []metrics.Value, color Color) {
	if w < 1 {
		return
	}
	values = values[max(0, len(values)-w*2):]
	for i := 0; i < w; i++ {
		bits := 0
		for sub := 0; sub < 2; sub++ {
			j := len(values) - w*2 + i*2 + sub
			if j < 0 || j >= len(values) || !values[j].OK {
				continue
			}
			level := min(3, max(0, int(math.Round(values[j].N*3/100))))
			left := []int{6, 2, 1, 0}
			right := []int{7, 5, 4, 3}
			if sub == 0 {
				bits |= 1 << left[3-level]
			} else {
				bits |= 1 << right[3-level]
			}
		}
		ch := '─'
		if bits > 0 {
			ch = rune(0x2800 + bits)
		}
		c.Put(x+i, y, ch, color, Background)
	}
}
func diagnostic(c *Canvas, s metrics.Snapshot, st State) {
	x, rail := frame(c, st, "DIAGNOSTIC / ENGINEERING ANALYSIS")
	large := c.W >= 105 && c.H >= 34
	c.Write(x, 5, s.OS+" / "+s.Arch, Amber)
	c.Text(x, 7, c.W-x-3, "HOST "+s.Host, Text, Background)
	c.Write(x, 9, "SWAP  "+bytes(s.SwapUsed)+" / "+bytes(s.SwapTotal), Lavender)
	c.Write(c.W-28, 5, "PROCESSES / "+val(s.ProcessCount, "%.0f"), Blue)
	c.Write(c.W-28, 7, "CPU  "+val(s.CPU, "%.1f%%"), Text)
	c.Write(c.W-28, 9, "UP   "+up(s.Uptime), Muted)
	c.Fill(rail, 11, c.W-rail-3, 1, Blue)
	c.Text(x, 11, 40, "04 / PROCESS ACTIVITY", Background, Blue)
	c.Write(x, 13, "PID", Muted)
	c.Write(x+9, 13, "COMMAND", Muted)
	c.Write(c.W-28, 13, "CPU %", Muted)
	c.Write(c.W-16, 13, "RESIDENT", Muted)
	for i, p := range s.Top {
		y := 15 + i
		if large {
			y = 15 + i*2
		}
		c.Write(x, y, fmt.Sprintf("%d", p.PID), Text)
		c.Text(x+9, y, c.W-x-40, p.Name, Text, Background)
		c.Write(c.W-28, y, val(p.CPU, "%6.1f"), Amber)
		c.Write(c.W-16, y, bytes(p.Memory), Lavender)
	}
	if len(s.Top) == 0 {
		c.Write(x, 15, "Process telemetry unavailable or awaiting sample", Muted)
	}
	if large {
		c.Write(x, 26, "05 / CORE ARRAY", Muted)
		cols := max(1, (c.W-x-4)/12)
		rows := max(1, c.H-32)
		capacity := cols * rows
		for i, v := range s.Cores {
			if i >= capacity {
				break
			}
			c.Write(x+(i%cols)*12, 28+i/cols, fmt.Sprintf("%02d %s", i, val(v, "%5.1f%%")), Blue)
		}
		if len(s.Cores) > capacity {
			c.Write(c.W-28, 26, fmt.Sprintf("+%d CORES / RESIZE", len(s.Cores)-capacity), Muted)
		}
	} else {
		c.Write(x, 20, "CORE", Muted)
		cols := max(1, (c.W-x-20)/12)
		for i, v := range s.Cores {
			if i >= cols {
				break
			}
			c.Write(x+6+i*12, 20, fmt.Sprintf("%02d %s", i, val(v, "%4.0f%%")), Blue)
		}
		if len(s.Cores) > cols {
			c.Write(c.W-12, 20, fmt.Sprintf("+%d", len(s.Cores)-cols), Muted)
		}
	}
	c.Write(x, c.H-1, "CPU: 100% = ONE CORE   /   p RETURN TO COMMAND", Muted)
}
func help(c *Canvas) {
	w := min(65, c.W-6)
	h := 16
	x := (c.W - w) / 2
	y := (c.H - h) / 2
	c.Fill(x-1, y-1, w+2, h+2, Background)
	pill(c, x, y, w, Orange, "COMMAND REFERENCE / ? CLOSE")
	lines := []string{"q / Esc    End terminal session", "?          Open / close this reference", "Space      Hold / resume telemetry", "p          Open / close diagnostics", "Tab        Switch command / diagnostic view", "", "HOLD freezes time, counters and animation.", "-- means unavailable or awaiting a baseline.", "CPU uses interval deltas; process 100% = one core.", "Network sums interfaces, excluding loopback.", "Resize for more telemetry. No root access needed."}
	for i, line := range lines {
		c.Text(x+2, y+2+i, w-4, line, Text, Background)
	}
}
func tiny(c *Canvas, s metrics.Snapshot, st State) {
	c.Fill(0, 0, c.W, 1, Orange)
	c.Text(1, 0, c.W-2, "LCARS / COMPACT COMMAND", Background, Orange)
	lines := []string{s.At.Local().Format(time.DateTime), "HOST " + s.Host, "CPU  " + val(s.CPU, "%.1f%%"), "MEM  " + bytes(used(s)) + " / " + bytes(s.MemTotal), "LOAD " + val(s.Load1, "%.2f"), "ROOT " + val(percent(s.DiskUsed, s.DiskTotal), "%.0f%%"), "RX   " + bytes(s.RX) + "/s", "TX   " + bytes(s.TX) + "/s", "Resize for command display"}
	if st.Diagnostic {
		lines = append([]string{s.OS + " / " + s.Arch, "PROCS " + val(s.ProcessCount, "%.0f"), "SWAP " + bytes(s.SwapUsed)}, lines...)
	}
	if st.Help {
		lines = []string{"q/Esc quit", "? help", "Space pause", "p diagnostics", "Tab change view"}
	}
	for i, line := range lines {
		if i+2 >= c.H-1 {
			break
		}
		c.Text(1, i+2, c.W-2, line, Text, Background)
	}
	c.Fill(0, c.H-1, c.W, 1, Lavender)
	c.Text(1, c.H-1, c.W-2, "q EXIT  ? HELP  p DIAG", Background, Lavender)
}
