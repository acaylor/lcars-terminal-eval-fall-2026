// Package metrics collects system metrics behind a stable Sampler
// interface. OS-specific files implement Sample(); all parsing helpers are
// platform-independent and unit-tested.
//
// Data flow: Sample() -> Snapshot (cumulative counters + a rate-calculated
// Rates block). The first sample carries no Rates; renderers must tolerate
// that. A metric that cannot be collected is reported as available=false
// rather than omitted, so the UI can show a tasteful "N/A".
package metrics

import "time"

type CPUCore struct {
	Label string
	Pct   float64 // 0..100
}

type Memory struct {
	Total, Available, Used uint64
	SwapTotal, SwapFree    uint64
	SwapAvailable          bool
}

type Disk struct {
	Path              string
	Total, Used, Free uint64
}

type NetTotals struct {
	Rx, Tx uint64
	Ifaces int
}

type Process struct {
	PID  int
	Name string // may contain arbitrary bytes; renderers must sanitize
	CPU  float64
	Mem  uint64
}

type Rates struct {
	CPUPct    float64
	CPUPctOK  bool
	Cores     []CPUCore
	NetRx     uint64 // bytes per second
	NetTx     uint64
	NetOK     bool
	Procs     []Process
	ProcsOK   bool
	Elapsed   time.Duration
	ProcCount int
}

type Snapshot struct {
	At       time.Time
	Hostname string
	OS, Arch string
	Uptime   time.Duration

	CPUCounters  []uint64 // cumulative jiffies/ticks: user nice sys idle iowait irq softirq steal
	CoreCounters [][]uint64
	Load         [3]float64
	LoadOK       bool

	Mem    Memory
	MemOK  bool
	Disk   Disk
	DiskOK bool
	Net    NetTotals
	NetOK  bool

	// Cumulative CPU time (ticks) for the processes seen this sample, used
	// to derive per-process CPU% on the next sample.
	procTicks     map[int]uint64
	procCount     int
	procInfoCache map[int]Process
}

type Sampler interface {
	Sample() Snapshot
	Close()
}
