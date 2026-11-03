package ui

import (
	"fmt"
	"math"
	"strings"
	"time"

	"lcars/internal/metrics"
)

type State struct {
	Snapshot                         metrics.Snapshot
	Now                              time.Time
	Diagnostics, Help, Paused, ASCII bool
	Tick                             int
	History                          []metrics.Value
	CorePage                         int
}

func Render(w, h int, s State) *Canvas {
	c := NewCanvas(w, h, s.ASCII)
	if c.W < 60 || c.H < 20 {
		tiny(c, s)
		return c
	}
	if s.Diagnostics {
		diagnostic(c, s)
	} else if c.W >= 116 && c.H >= 32 {
		command(c, s)
	} else {
		compact(c, s)
	}
	if s.Help {
		help(c)
	}
	return c
}
func bytes(n uint64) string {
	units := []string{"B", "KiB", "MiB", "GiB", "TiB", "PiB"}
	value := float64(n)
	i := 0
	for value >= 1024 && i < len(units)-1 {
		value /= 1024
		i++
	}
	if i == 0 {
		return fmt.Sprintf("%d B", n)
	}
	return fmt.Sprintf("%.1f %s", value, units[i])
}
func percent(v metrics.Value) string {
	if !v.OK {
		return "--"
	}
	return fmt.Sprintf("%.1f", v.N)
}
func rate(v metrics.Value) string {
	if !v.OK {
		return "-- /s"
	}
	return bytes(uint64(max(0, v.N))) + "/s"
}
func capUsed(v metrics.Capacity) string {
	if !v.OK {
		return "unavailable"
	}
	return bytes(v.Used) + " / " + bytes(v.Total)
}
func capAvail(v metrics.Capacity) string {
	if !v.OK {
		return "-- available"
	}
	return bytes(v.Available) + " available"
}
func uptime(s metrics.Snapshot) string {
	if !s.UptimeOK {
		return "--"
	}
	n := int64(s.Uptime.Seconds())
	return fmt.Sprintf("%02dd %02dh %02dm", n/86400, n/3600%24, n/60%60)
}
func loads(s metrics.Snapshot) string {
	if !s.LoadOK {
		return "-- / -- / --"
	}
	return fmt.Sprintf("%.2f / %.2f / %.2f", s.Load[0], s.Load[1], s.Load[2])
}
func status(s State) (string, Color) {
	if s.Paused {
		return "HOLD", Lavender
	}
	if s.Snapshot.At.IsZero() {
		return "ACQUIRING", Muted
	}
	if len(s.Snapshot.Issues) > 0 || !s.Snapshot.Memory.OK || !s.Snapshot.Disk.OK {
		return "PARTIAL", Salmon
	}
	if s.Snapshot.Usage.OK && s.Snapshot.Usage.N >= 90 {
		return "HIGH LOAD", Salmon
	}
	return "ONLINE", Blue
}
func frame(c *Canvas, s State, title string) {
	c.Pill(2, 1, 13, Orange, "47 / SYS")
	c.Text(18, 1, "LCARS", Amber)
	c.Text(25, 1, "/ "+title, Muted)
	label, color := status(s)
	c.Pill(c.W-19, 1, 17, color, label)
	c.Text(3, c.H-2, "01", Orange)
	c.ClipText(9, c.H-2, c.W-10, "[p] diagnostics   [tab] view   [space] hold   [?] help   [q] exit", Muted)
}

func command(c *Canvas, s State) {
	frame(c, s, "ENGINEERING")
	w, h := c.W, c.H
	// One continuous upper elbow establishes the silhouette, with a lower return rail.
	c.Elbow(2, 3, w-6, 7, 12, 3, Orange, false)
	c.Label(4, 5, "COMMAND", Background, Orange)
	c.Label(7, 8, "01-A", Background, Orange)
	c.Fill(2, 11, 12, 4, Lavender)
	c.Label(8, 13, "080", Background, Lavender)
	c.Fill(2, 16, 12, 3, Salmon)
	c.Label(8, 17, "214", Background, Salmon)
	c.Fill(2, 20, 12, h-26, Blue)
	c.Label(7, 21, "NET", Background, Blue)
	c.Elbow(2, h-6, 53, 3, 12, 1, Orange, true)
	c.Pill(57, h-4, 20, Salmon, "LOCAL / 01")
	c.Pill(79, h-4, w-81, Lavender, "COMPUTER ACCESS")
	c.Text(19, 8, "CHRONOMETRY", Orange)
	c.ClipText(76, 8, w-80, strings.ToUpper(s.Now.Format("Mon / 02 Jan 2006")), Muted)
	c.Digits(18, 10, s.Now.Format("15:04:05"), Amber, 2)
	c.Text(19, 16, "LOCAL TIME", Muted)
	c.ClipText(44, 16, w-48, "HOST / "+strings.ToUpper(s.Snapshot.Host), Lavender)
	c.Text(19, 19, "01 / COMPUTE", Orange)
	load := "--"
	if s.Snapshot.LoadOK {
		load = fmt.Sprintf("%.2f", s.Snapshot.Load[0])
	}
	if len(load) > 5 {
		load = fmt.Sprintf("%.0f", s.Snapshot.Load[0])
	}
	if len(load) > 6 {
		load = "--"
	}
	c.Digits(19, 21, load, Lavender, 1)
	c.Text(19, 27, "LOAD / 1 MIN", Muted)
	c.Text(48, 21, percent(s.Snapshot.Usage)+"%", Amber)
	c.Text(48, 22, "CPU ACTIVE", Muted)
	c.Text(48, 24, "5M / 15M", Muted)
	if s.Snapshot.LoadOK {
		c.Text(48, 25, fmt.Sprintf("%.2f / %.2f", s.Snapshot.Load[1], s.Snapshot.Load[2]), Lavender)
	} else {
		c.Text(48, 25, "-- / --", Muted)
	}
	x := max(74, w-45)
	c.Pill(x, 19, w-x-4, Blue, "02 / RESOURCE TELEMETRY")
	c.Text(x, 21, "MEM", Blue)
	c.ClipText(x+6, 21, w-x-10, capUsed(s.Snapshot.Memory), White)
	c.Text(x+6, 22, capAvail(s.Snapshot.Memory), Muted)
	c.Text(x, 24, "ROOT", Salmon)
	c.ClipText(x+6, 24, w-x-10, capUsed(s.Snapshot.Disk), White)
	c.Text(x+6, 25, capAvail(s.Snapshot.Disk), Muted)
	c.Text(x, 27, "RX", Blue)
	c.Text(x+4, 27, rate(s.Snapshot.RX), White)
	c.Text(x+21, 27, "TX", Salmon)
	c.ClipText(x+25, 27, w-x-29, rate(s.Snapshot.TX), White)
	if h >= 36 {
		trace(c, 19, 29, 44, s.History, Blue)
		c.Text(68, 29, "UP / "+uptime(s.Snapshot), Muted)
		c.Put(w-6, 29, []rune{'◉', '●', '◉', '○'}[s.Tick%4], Blue, Background)
	} else {
		c.Text(48, 27, "UP "+uptime(s.Snapshot), Muted)
	}
}

func compact(c *Canvas, s State) {
	frame(c, s, "COMMAND")
	c.Elbow(2, 3, c.W-5, 6, 8, 1, Orange, false)
	c.Label(4, 6, "01-A", Background, Orange)
	c.Fill(2, 10, 8, 4, Lavender)
	c.Label(4, 12, "CPU", Background, Lavender)
	c.Fill(2, 15, 8, c.H-19, Blue)
	c.Fill(2, c.H-4, 8, 1, Salmon)
	c.Fill(10, c.H-4, c.W-13, 1, Salmon)
	c.Digits(14, 5, s.Now.Format("15:04:05"), Amber, 1)
	c.ClipText(54, 5, c.W-57, strings.ToUpper(s.Now.Format("02 Jan 2006")), Muted)
	c.ClipText(54, 7, c.W-57, strings.ToUpper(s.Snapshot.Host), Lavender)
	c.Text(14, 11, "CPU "+percent(s.Snapshot.Usage)+"%", Amber)
	c.ClipText(34, 11, c.W-37, "LOAD "+loads(s.Snapshot), Lavender)
	c.Text(14, 13, "MEM  "+capUsed(s.Snapshot.Memory), Blue)
	c.ClipText(49, 13, c.W-52, capAvail(s.Snapshot.Memory), Muted)
	c.Text(14, 15, "/    "+capUsed(s.Snapshot.Disk), Salmon)
	c.ClipText(49, 15, c.W-52, capAvail(s.Snapshot.Disk), Muted)
	if c.H >= 23 {
		c.Text(14, 17, "RX "+rate(s.Snapshot.RX), Blue)
		c.ClipText(44, 17, c.W-47, "TX "+rate(s.Snapshot.TX), Salmon)
		c.Text(14, 19, "UP / "+uptime(s.Snapshot), Muted)
	} else {
		c.ClipText(14, 17, c.W-17, "RX "+rate(s.Snapshot.RX)+"  TX "+rate(s.Snapshot.TX), Blue)
	}
}

func diagnostic(c *Canvas, s State) {
	frame(c, s, "DIAGNOSTICS")
	w, h := c.W, c.H
	c.Fill(2, 4, 6, h-8, Lavender)
	c.Fill(8, 4, w-11, 1, Lavender)
	c.Label(3, 6, "02", Background, Lavender)
	c.Label(3, h-6, "DX", Background, Lavender)
	c.ClipText(12, 6, w-16, s.Snapshot.OS+" / "+s.Snapshot.Arch, White)
	count := "--"
	if s.Snapshot.ProcessesOK {
		count = fmt.Sprint(s.Snapshot.ProcessCount)
	}
	c.ClipText(12, 8, w-16, "PROCESSES / "+count+"     SWAP / "+capUsed(s.Snapshot.Swap), Muted)
	coreStart, coreRows := 12, max(1, min(7, h-23))
	if h < 28 {
		coreStart = 10
		coreRows = 2
	}
	cols := max(1, (w-16)/17)
	pageSize := cols * coreRows
	pages := max(1, (len(s.Snapshot.Cores)+pageSize-1)/pageSize)
	page := s.CorePage % pages
	headerY := 10
	if h < 28 {
		headerY = 9
	}
	c.Text(12, headerY, fmt.Sprintf("PROCESSOR ARRAY / %d CORES", len(s.Snapshot.Cores)), Orange)
	if w >= 90 {
		c.Text(w-35, 10, fmt.Sprintf("PAGE %d/%d  [ / ]", page+1, pages), Muted)
	}
	for i := 0; i < pageSize; i++ {
		idx := page*pageSize + i
		if idx >= len(s.Snapshot.Cores) {
			break
		}
		core := s.Snapshot.Cores[idx]
		x, y := 12+(i%cols)*17, coreStart+i/cols
		c.ClipText(x, y, 4, fmt.Sprintf("%03s", strings.TrimPrefix(core.ID, "cpu")), Muted)
		c.Text(x+5, y, fmt.Sprintf("%5s%%", percent(core.Usage)), Blue)
		if core.Usage.OK {
			c.Put(x+13, y, []rune{'▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'}[min(7, int(core.Usage.N*8/101))], Lavender, Background)
		}
	}
	if len(s.Snapshot.Cores) == 0 {
		c.Text(12, coreStart, "Core telemetry unavailable", Muted)
	}
	y := coreStart + coreRows + 2
	c.Pill(11, y, w-14, Orange, "CPU / FIVE MOST ACTIVE")
	c.Text(13, y+2, "PID", Muted)
	c.Text(24, y+2, "PROCESS", Muted)
	c.Text(w-25, y+2, "CPU %", Muted)
	c.Text(w-14, y+2, "RESIDENT", Muted)
	for i, p := range s.Snapshot.Top {
		row := y + 3 + i
		if row >= h-2 {
			break
		}
		c.Text(13, row, fmt.Sprintf("%-9d", p.PID), Lavender)
		c.ClipText(24, row, w-51, p.Name, White)
		c.Text(w-25, row, fmt.Sprintf("%6s", percent(p.CPU)), Amber)
		c.Text(w-14, row, bytes(p.Memory), Blue)
	}
	if len(s.Snapshot.Top) == 0 {
		c.Text(13, y+3, "Waiting for accessible process samples", Muted)
	}
	if h >= 32 {
		c.Text(12, h-4, "CPU: 100% = one core   |   denied or exited processes are omitted", Muted)
	}
}

func tiny(c *Canvas, s State) {
	c.Fill(0, 0, c.W, 1, Orange)
	c.Label(1, 0, "LCARS / COMPACT", Background, Orange)
	c.ClipText(1, 2, c.W-2, s.Now.Format("15:04:05  02 Jan 2006"), Amber)
	c.ClipText(1, 4, c.W-2, "CPU "+percent(s.Snapshot.Usage)+"%  UP "+uptime(s.Snapshot), Blue)
	c.ClipText(1, 6, c.W-2, "Resize to 60 x 20 for command view", Muted)
	c.ClipText(1, c.H-2, c.W-2, "q exit / ? help / space hold", Muted)
	if s.Help && c.H > 9 {
		c.ClipText(1, 8, c.W-2, "p or tab: diagnostics", Lavender)
	}
}

func help(c *Canvas) {
	w, h := min(66, c.W-4), min(17, c.H-2)
	x, y := (c.W-w)/2, (c.H-h)/2
	c.Fill(x, y, w, h, Background)
	c.Fill(x, y, 4, h, Lavender)
	c.Fill(x+4, y, w-4, 2, Lavender)
	c.Label(x+7, y, "COMPUTER / OPERATOR REFERENCE", Background, Lavender)
	lines := []string{"q / Esc     Leave command display", "?           Open / close this reference", "Space       Hold / resume telemetry and clock", "p / Tab     Command / process diagnostics", "[ / ]       Previous / next core page", "", "--          Acquiring or unavailable telemetry", "RX / TX     All non-loopback interfaces", "CPU         Process 100% = one logical core", "", "Press ? to return"}
	for i, line := range lines {
		if i+4 < h {
			c.ClipText(x+7, y+3+i, w-9, line, White)
		}
	}
}

func trace(c *Canvas, x, y, w int, values []metrics.Value, color Color) {
	if c.ASCII {
		for i := 0; i < min(w, len(values)); i++ {
			if values[len(values)-min(w, len(values))+i].OK {
				c.Put(x+i, y, '.', color, Background)
			}
		}
		return
	}
	points := min(len(values), w*2)
	start := len(values) - points
	for col := 0; col < w; col++ {
		var dots rune
		for sub := 0; sub < 2; sub++ {
			idx := col*2 + sub - (w*2 - points)
			if idx < 0 {
				continue
			}
			v := values[start+idx]
			if !v.OK {
				continue
			}
			level := 3 - int(math.Round(min(100, max(0, v.N))*3/100))
			masks := [2][4]rune{{1, 2, 4, 64}, {8, 16, 32, 128}}
			dots |= masks[sub][level]
		}
		if dots != 0 {
			c.Put(x+col, y, rune(0x2800)+dots, color, Background)
		}
	}
}
