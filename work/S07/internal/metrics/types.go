// Package metrics isolates OS collection from deterministic interval calculations.
package metrics

import (
	"math"
	"sort"
	"time"
)

type Value struct {
	N  float64
	OK bool
}
type Capacity struct {
	Total, Used, Available uint64
	OK                     bool
}
type CPUCounter struct{ Total, Idle uint64 }
type NetCounter struct{ RX, TX uint64 }
type Process struct {
	PID     int
	Name    string
	Start   uint64
	Seconds float64
	Memory  uint64
	CPU     Value
}
type Raw struct {
	At                 time.Time
	Host, OS, Arch     string
	Uptime             time.Duration
	UptimeOK           bool
	CPU                map[string]CPUCounter
	Load               [3]float64
	LoadOK             bool
	Memory, Swap, Disk Capacity
	Network            map[string]NetCounter
	Processes          map[int]Process
	ProcessCount       int
	ProcessesOK        bool
	Issues             []string
}
type Core struct {
	ID    string
	Usage Value
}
type Snapshot struct {
	Raw
	Usage, RX, TX Value
	Cores         []Core
	Top           []Process
}

type Collector interface{ Collect(details bool) Raw }

// Engine owns the previous sample. A missed sample cannot produce an invented zero.
type Engine struct{ previous *Raw }

func Rate(before, after uint64, elapsed time.Duration) Value {
	if elapsed <= 0 || elapsed > 30*time.Second || after < before {
		return Value{}
	}
	n := float64(after-before) / elapsed.Seconds()
	return Value{n, !math.IsNaN(n) && !math.IsInf(n, 0)}
}

func CPUPercent(before, after CPUCounter) Value {
	if after.Total <= before.Total || after.Idle < before.Idle {
		return Value{}
	}
	total, idle := after.Total-before.Total, after.Idle-before.Idle
	if idle > total {
		return Value{}
	}
	return Value{100 * float64(total-idle) / float64(total), true}
}

func (e *Engine) Sample(raw Raw) Snapshot {
	s := Snapshot{Raw: raw}
	previous := e.previous
	e.previous = &raw
	validInterval := previous != nil && raw.At.Sub(previous.At) > 0 && raw.At.Sub(previous.At) <= 30*time.Second
	ids := make([]string, 0, len(raw.CPU))
	for id := range raw.CPU {
		if id != "cpu" {
			ids = append(ids, id)
		}
	}
	sort.Slice(ids, func(i, j int) bool {
		if len(ids[i]) != len(ids[j]) {
			return len(ids[i]) < len(ids[j])
		}
		return ids[i] < ids[j]
	})
	usage := func(id string) Value {
		if !validInterval {
			return Value{}
		}
		a, aok := previous.CPU[id]
		b, bok := raw.CPU[id]
		if !aok || !bok {
			return Value{}
		}
		return CPUPercent(a, b)
	}
	s.Usage = usage("cpu")
	for _, id := range ids {
		s.Cores = append(s.Cores, Core{id, usage(id)})
	}
	if validInterval {
		for id, counter := range raw.Network {
			old, ok := previous.Network[id]
			if !ok {
				continue
			}
			rx, tx := Rate(old.RX, counter.RX, raw.At.Sub(previous.At)), Rate(old.TX, counter.TX, raw.At.Sub(previous.At))
			if rx.OK && tx.OK {
				s.RX.N += rx.N
				s.TX.N += tx.N
				s.RX.OK = true
				s.TX.OK = true
			}
		}
	}
	for _, p := range raw.Processes {
		if validInterval {
			if old, ok := previous.Processes[p.PID]; ok && old.Start == p.Start && p.Seconds >= old.Seconds {
				p.CPU = Value{100 * (p.Seconds - old.Seconds) / raw.At.Sub(previous.At).Seconds(), true}
			}
		}
		s.Top = append(s.Top, p)
	}
	sort.Slice(s.Top, func(i, j int) bool {
		a, b := s.Top[i], s.Top[j]
		if a.CPU.OK != b.CPU.OK {
			return a.CPU.OK
		}
		if a.CPU.N != b.CPU.N {
			return a.CPU.N > b.CPU.N
		}
		return a.PID < b.PID
	})
	if len(s.Top) > 5 {
		s.Top = s.Top[:5]
	}
	return s
}
