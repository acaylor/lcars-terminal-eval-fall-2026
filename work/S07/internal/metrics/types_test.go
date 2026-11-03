package metrics

import (
	"math"
	"testing"
	"time"
)

func TestRateBoundaries(t *testing.T) {
	for _, tc := range []struct {
		name          string
		before, after uint64
		elapsed       time.Duration
		want          Value
	}{
		{"elapsed", 100, 400, 1500 * time.Millisecond, Value{200, true}},
		{"zero traffic", 100, 100, time.Second, Value{0, true}},
		{"reset", 400, 100, time.Second, Value{}},
		{"rollover", math.MaxUint64, 1, time.Second, Value{}},
		{"zero interval", 0, 1, 0, Value{}},
		{"backwards", 0, 1, -time.Second, Value{}},
		{"suspension", 0, 1, 31 * time.Second, Value{}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := Rate(tc.before, tc.after, tc.elapsed); got != tc.want {
				t.Fatalf("got %+v want %+v", got, tc.want)
			}
		})
	}
}
func TestCPUPercent(t *testing.T) {
	if got := CPUPercent(CPUCounter{100, 80}, CPUCounter{300, 180}); got != (Value{50, true}) {
		t.Fatal(got)
	}
	for _, pair := range [][2]CPUCounter{{{100, 80}, {99, 90}}, {{100, 80}, {200, 79}}, {{100, 80}, {200, 190}}, {{100, 80}, {100, 80}}} {
		if got := CPUPercent(pair[0], pair[1]); got.OK {
			t.Fatal(got)
		}
	}
}
func TestSamplingChurnAndIdentity(t *testing.T) {
	var e Engine
	first := Raw{At: time.Unix(10, 0), CPU: map[string]CPUCounter{"cpu": {100, 50}, "cpu0": {100, 50}}, Network: map[string]NetCounter{"eth0/2": {100, 200}, "gone": {1000, 2000}}, Processes: map[int]Process{1: {PID: 1, Start: 1, Seconds: 1}, 2: {PID: 2, Start: 2, Seconds: 5}, 3: {PID: 3, Start: 3, Seconds: 5}}}
	if got := e.Sample(first); got.Usage.OK || got.RX.OK || got.Top[0].CPU.OK {
		t.Fatal("first sample invented rates")
	}
	second := Raw{At: first.At.Add(2 * time.Second), CPU: map[string]CPUCounter{"cpu": {300, 100}, "cpu0": {300, 100}, "cpu1": {10, 1}}, Network: map[string]NetCounter{"eth0/2": {500, 800}, "new": {999999, 999999}}, Processes: map[int]Process{1: {PID: 1, Start: 1, Seconds: 4}, 2: {PID: 2, Start: 99, Seconds: 900}, 4: {PID: 4, Start: 4, Seconds: 999}}}
	got := e.Sample(second)
	if got.Usage != (Value{75, true}) || got.RX != (Value{200, true}) || got.TX != (Value{300, true}) {
		t.Fatalf("wrong rates: %+v", got)
	}
	if got.Top[0].PID != 1 || got.Top[0].CPU != (Value{150, true}) {
		t.Fatalf("CPU must use elapsed time, with 100%% per core: %+v", got.Top)
	}
	if got.Top[1].CPU.OK || got.Top[2].CPU.OK || got.Cores[1].Usage.OK {
		t.Fatal("new identities need baselines")
	}
	second.At = second.At.Add(time.Minute)
	if got := e.Sample(second); got.Usage.OK || got.RX.OK || got.Top[0].CPU.OK {
		t.Fatal("long interval should rebaseline")
	}
}
func TestTopFiveAndMissingCollectors(t *testing.T) {
	var e Engine
	r := Raw{At: time.Now(), Processes: map[int]Process{}}
	for i := 1; i <= 12; i++ {
		r.Processes[i] = Process{PID: i, Start: uint64(i)}
	}
	e.Sample(r)
	r2 := Raw{At: r.At.Add(time.Second), Processes: map[int]Process{}}
	for i, p := range r.Processes {
		p.Seconds = float64(i)
		r2.Processes[i] = p
	}
	s := e.Sample(r2)
	if len(s.Top) != 5 || s.Top[0].PID != 12 || s.Top[4].PID != 8 {
		t.Fatal(s.Top)
	}
	if s.Usage.OK || s.RX.OK {
		t.Fatal("missing collector reported a value")
	}
	if got := e.Sample(Raw{At: r2.At.Add(time.Second)}); len(got.Top) != 0 {
		t.Fatal("disappeared processes retained")
	}
}
