// Package metrics separates platform counters from interval calculations.
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

func Known(n float64) Value { return Value{n, !math.IsNaN(n) && !math.IsInf(n, 0)} }

type CPU struct{ Busy, Total uint64 }
type Network struct{ RX, TX uint64 }
type Process struct {
	PID        int
	Name       string
	CPUSeconds float64
	Memory     uint64
	Identity   uint64
	MemoryOK   bool
}
type ProcessView struct {
	PID    int
	Name   string
	CPU    Value
	Memory Value
}
type Raw struct {
	At                                                               time.Time
	Host, OS, Arch                                                   string
	Uptime, Load1, Load5, Load15                                     Value
	MemTotal, MemAvailable, DiskTotal, DiskUsed, SwapTotal, SwapUsed Value
	CPUs                                                             []CPU
	Network                                                          map[string]Network
	Processes                                                        []Process
	ProcessCount                                                     Value
	// Issues contains names of failed collectors, never user-controlled error text.
	Issues []string
}
type Snapshot struct {
	Raw
	CPU, RX, TX Value
	Cores       []Value
	Top         []ProcessView
}
type Collector interface{ Collect(processes bool) Raw }
type Sampler struct {
	previous     Raw
	havePrevious bool
}

// Delta rejects reset counters; a wrapped counter has no trustworthy rate.
func Delta(before, after uint64, elapsed float64) Value {
	if elapsed <= 0 || math.IsNaN(elapsed) || math.IsInf(elapsed, 0) || after < before {
		return Value{}
	}
	return Known(float64(after-before) / elapsed)
}
func utilization(a, b CPU) Value {
	if b.Total <= a.Total || b.Busy < a.Busy {
		return Value{}
	}
	return Known(math.Min(100, 100*float64(b.Busy-a.Busy)/float64(b.Total-a.Total)))
}
func (s *Sampler) Sample(r Raw) Snapshot {
	out := Snapshot{Raw: r, Cores: make([]Value, max(0, len(r.CPUs)-1))}
	dt := r.At.Sub(s.previous.At).Seconds()
	if s.havePrevious && dt > 0 {
		if len(r.CPUs) > 0 && len(s.previous.CPUs) > 0 {
			out.CPU = utilization(s.previous.CPUs[0], r.CPUs[0])
		}
		for i := range out.Cores {
			if i+1 < len(s.previous.CPUs) {
				out.Cores[i] = utilization(s.previous.CPUs[i+1], r.CPUs[i+1])
			}
		}
		if r.Network != nil && s.previous.Network != nil {
			// Match interfaces individually: hotplug cannot create a spike.
			var rx, tx float64
			valid := len(r.Network) == 0 && len(s.previous.Network) == 0
			for name, b := range r.Network {
				if a, ok := s.previous.Network[name]; ok {
					x, y := Delta(a.RX, b.RX, dt), Delta(a.TX, b.TX, dt)
					if x.OK && y.OK {
						rx += x.N
						tx += y.N
						valid = true
					}
				}
			}
			if valid {
				out.RX = Known(rx)
				out.TX = Known(tx)
			}
		}
	}
	prior := make(map[int]Process, len(s.previous.Processes))
	for _, p := range s.previous.Processes {
		prior[p.PID] = p
	}
	for _, p := range r.Processes {
		v := ProcessView{PID: p.PID, Name: p.Name}
		if p.MemoryOK {
			v.Memory = Known(float64(p.Memory))
		}
		if a, ok := prior[p.PID]; ok && a.Identity == p.Identity && dt > 0 && p.CPUSeconds >= a.CPUSeconds {
			v.CPU = Known(100 * (p.CPUSeconds - a.CPUSeconds) / dt)
		}
		out.Top = append(out.Top, v)
	}
	sort.Slice(out.Top, func(i, j int) bool {
		a, b := out.Top[i], out.Top[j]
		if a.CPU.OK != b.CPU.OK {
			return a.CPU.OK
		}
		if a.CPU.N != b.CPU.N {
			return a.CPU.N > b.CPU.N
		}
		return a.PID < b.PID
	})
	if len(out.Top) > 5 {
		out.Top = out.Top[:5]
	}
	// Own the baseline so collectors can safely reuse their slice/map buffers.
	s.previous = r
	s.previous.CPUs = append([]CPU(nil), r.CPUs...)
	s.previous.Processes = append([]Process(nil), r.Processes...)
	if r.Network != nil {
		s.previous.Network = make(map[string]Network, len(r.Network))
		for name, counters := range r.Network {
			s.previous.Network[name] = counters
		}
	}
	s.havePrevious = true
	return out
}
