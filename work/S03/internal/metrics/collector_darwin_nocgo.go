//go:build darwin && !cgo

package metrics

import "time"

type unavailableCollector struct{}

func NewCollector() Collector { return unavailableCollector{} }
func (unavailableCollector) Collect(bool) Raw {
	return Raw{At: time.Now(), OS: "macOS", Issues: []string{"native APIs require a build with CGO_ENABLED=1"}}
}
