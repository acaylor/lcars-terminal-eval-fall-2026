package metrics

import (
	"math"
	"testing"
	"time"
)

func TestDelta(t *testing.T) {
	for _, tc := range []struct {
		name     string
		a, b     uint64
		dt, want float64
		ok       bool
	}{
		{"real elapsed", 100, 250, 2.5, 60, true}, {"long gap", 0, 100, 1000, .1, true},
		{"reset", 100, 10, 1, 0, false}, {"zero", 0, 100, 0, 0, false}, {"negative", 0, 100, -1, 0, false},
		{"nan", 0, 100, math.NaN(), 0, false}, {"infinity", 0, 100, math.Inf(1), 0, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			v := Delta(tc.a, tc.b, tc.dt)
			if v.OK != tc.ok || math.Abs(v.N-tc.want) > 1e-9 {
				t.Fatalf("got %+v", v)
			}
		})
	}
}
func TestSamplerIntervalsHotplugAndPIDReuse(t *testing.T) {
	at := time.Unix(100, 0)
	sampler := Sampler{}
	a := Raw{At: at, CPUs: []CPU{{50, 100}, {25, 50}, {25, 50}}, Network: map[string]Network{"eth0": {100, 200}, "gone": {1 << 40, 1 << 40}}, Processes: []Process{{PID: 1, Identity: 7, CPUSeconds: 2}, {PID: 2, Identity: 9, CPUSeconds: 100}}}
	first := sampler.Sample(a)
	if first.CPU.OK || first.RX.OK || first.Top[0].CPU.OK {
		t.Fatal("first sample invented rates")
	}
	b := Raw{At: at.Add(2 * time.Second), CPUs: []CPU{{100, 300}, {50, 150}, {50, 150}}, Network: map[string]Network{"eth0": {200, 260}, "new": {1 << 42, 1 << 42}}, Processes: []Process{{PID: 1, Identity: 7, CPUSeconds: 3}, {PID: 2, Identity: 10, CPUSeconds: 200}}}
	s := sampler.Sample(b)
	if s.CPU.N != 25 || s.RX.N != 50 || s.TX.N != 30 || len(s.Cores) != 2 || s.Cores[0].N != 25 {
		t.Fatalf("bad counters: %+v", s)
	}
	if s.Top[0].PID != 1 || s.Top[0].CPU.N != 50 || s.Top[1].CPU.OK {
		t.Fatalf("bad PID handling: %+v", s.Top)
	}
	b.At = b.At.Add(time.Second)
	b.CPUs = []CPU{{1, 2}}
	b.Network["eth0"] = Network{1, 2}
	b.Network["new"] = Network{1, 2}
	b.Processes = nil
	s = sampler.Sample(b)
	if s.CPU.OK || s.RX.OK || len(s.Top) != 0 {
		t.Fatal("reset/disappearance must invalidate counters")
	}
}
func TestSamplerZeroElapsedTopFiveAndRecovery(t *testing.T) {
	s := Sampler{}
	r := Raw{At: time.Unix(100, 0), CPUs: []CPU{{0, 1}}}
	for i := 0; i < 8; i++ {
		r.Processes = append(r.Processes, Process{PID: i + 1, Identity: 1})
	}
	s.Sample(r)
	for i := range r.Processes {
		r.Processes[i].CPUSeconds = float64(i)
	}
	zero := s.Sample(r)
	if zero.CPU.OK || zero.Top[0].CPU.OK || len(zero.Top) != 5 {
		t.Fatal("zero elapsed sample or bounded list failed")
	}
	r.At = r.At.Add(20 * time.Second)
	r.CPUs[0] = CPU{5, 21}
	for i := range r.Processes {
		r.Processes[i].CPUSeconds += float64(i)
	}
	next := s.Sample(r)
	if !next.CPU.OK || next.Top[0].PID != 8 || next.Top[0].CPU.N != 35 {
		t.Fatalf("recovery failed: %+v", next.Top)
	}
}
