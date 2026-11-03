package metrics

import (
	"sort"
	"strings"
	"time"
)

// cpuBusyPct converts two cumulative counter sets ([user nice sys idle iowait
// irq softirq steal]) into a utilization percentage. It tolerates counter
// resets (cur < prev), shrinking slices, and zero elapsed ticks.
func cpuBusyPct(prev, cur []uint64) (float64, bool) {
	n := len(cur)
	if len(prev) < n {
		n = len(prev)
	}
	if n < 4 {
		return 0, false
	}
	var totCur uint64
	var idleCur uint64
	for i := 0; i < n; i++ {
		if cur[i] < prev[i] {
			return 0, false // counter reset: no rate this interval
		}
		d := cur[i] - prev[i]
		if i == 3 || i == 4 {
			idleCur += d
		}
		totCur += d
	}
	if totCur == 0 {
		return 0, false
	}
	pct := 100 * float64(totCur-idleCur) / float64(totCur)
	if pct < 0 {
		pct = 0
	}
	if pct > 100 {
		pct = 100
	}
	return pct, true
}

// ComputeRates derives Rates from two snapshots. dt is the wall-clock
// elapsed time; zero/negative dt yields CPUPctOK=false rather than a panic.
func ComputeRates(prev, cur Snapshot) Rates {
	r := Rates{Elapsed: cur.At.Sub(prev.At)}
	if prev.At.IsZero() {
		return r
	}
	if p, ok := cpuBusyPct(prev.CPUCounters, cur.CPUCounters); ok {
		r.CPUPct, r.CPUPctOK = p, true
	}
	if len(cur.CoreCounters) == len(prev.CoreCounters) {
		for i := range cur.CoreCounters {
			p, ok := cpuBusyPct(prev.CoreCounters[i], cur.CoreCounters[i])
			if !ok {
				p = 0
			}
			lbl := "C" + itoaSmall(i+1)
			r.Cores = append(r.Cores, CPUCore{Label: lbl, Pct: p})
		}
	}
	if r.Elapsed > 0 && cur.NetOK && prev.NetOK {
		sec := r.Elapsed.Seconds()
		if cur.Net.Rx >= prev.Net.Rx {
			r.NetRx = uint64(float64(cur.Net.Rx-prev.Net.Rx) / sec)
		}
		if cur.Net.Tx >= prev.Net.Tx {
			r.NetTx = uint64(float64(cur.Net.Tx-prev.Net.Tx) / sec)
		}
		r.NetOK = true
	}
	// Process rates need name/RSS info; the assembler call fills Procs via
	// ComputeProcessRates using each platform's procInfoCache.
	r.ProcCount = cur.procCount
	return r
}

// ComputeProcessRates returns the top-n processes by CPU% from raw
// cumulative tick maps. Processes absent in prev start at 0. dt<=0 yields
// zero CPU for all (still sorted by memory). Output is deterministic:
// CPU desc, RSS desc, PID asc.
func ComputeProcessRates(prevTicks, curTicks map[int]uint64, info map[int]Process, dt time.Duration, n int) []Process {
	out := make([]Process, 0, len(curTicks))
	for pid, ticks := range curTicks {
		p := info[pid]
		p.PID = pid
		if dt > 0 {
			base := prevTicks[pid]
			if ticks >= base {
				p.CPU = 100 * float64(ticks-base) / (dt.Seconds() * float64(clockTicks()))
			}
		}
		out = append(out, p)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].CPU != out[j].CPU {
			return out[i].CPU > out[j].CPU
		}
		if out[i].Mem != out[j].Mem {
			return out[i].Mem > out[j].Mem
		}
		return out[i].PID < out[j].PID
	})
	if n > 0 && len(out) > n {
		out = out[:n]
	}
	return out
}

func clockTicks() uint64 { return 100 } // USER_HZ on Linux & macOS

func itoaSmall(i int) string {
	if i == 0 {
		return "0"
	}
	b := [8]byte{}
	p := len(b)
	for i > 0 {
		p--
		b[p] = byte('0' + i%10)
		i /= 10
	}
	return string(b[p:])
}

func formatUptime(d time.Duration) string {
	if d < 0 {
		d = 0
	}
	days := int(d.Hours()) / 24
	h := int(d.Hours()) % 24
	m := int(d.Minutes()) % 60
	if days > 0 {
		return itoaSmall(days) + "d " + pad2(h) + "h " + pad2(m) + "m"
	}
	return itoaSmall(h) + "h " + pad2(m) + "m"
}

func pad2(v int) string {
	if v < 10 {
		return "0" + itoaSmall(v)
	}
	return itoaSmall(v)
}

func formatRate(b uint64) string {
	switch {
	case b >= 1<<30:
		return ftoa(float64(b)/float64(1<<30)) + " GB/s"
	case b >= 1<<20:
		return ftoa(float64(b)/float64(1<<20)) + " MB/s"
	case b >= 1<<10:
		return ftoa(float64(b)/float64(1<<10)) + " KB/s"
	}
	return itoaSmall(int(b)) + " B/s"
}

func formatBytes(b uint64) string {
	switch {
	case b >= 1<<40:
		return ftoa(float64(b)/float64(1<<40)) + " TB"
	case b >= 1<<30:
		return ftoa(float64(b)/float64(1<<30)) + " GB"
	case b >= 1<<20:
		return ftoa(float64(b)/float64(1<<20)) + " MB"
	}
	return itoaSmall(int(b)) + " B"
}

func ftoa(f float64) string {
	// one decimal place
	i := int(f*10 + 0.5)
	return itoaSmall(i/10) + "." + itoaSmall(i%10)
}

// Sanitize replaces control characters, tabs and newlines with U+FFFD so
// untrusted process/host names cannot inject terminal escapes.
func Sanitize(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case r == '\x1b':
			b.WriteRune('\uFFFD')
		case r < 0x20 || r == 0x7f:
			b.WriteRune('\uFFFD')
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

// UptimeString, BytesString, RateString are exported formatting helpers.
func UptimeString(d time.Duration) string { return formatUptime(d) }
func BytesString(b uint64) string         { return formatBytes(b) }
func RateString(b uint64) string          { return formatRate(b) }
