.PHONY: build test lint captures perf
build:
	go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars ./cmd/lcars
test:
	go test -race ./...
lint:
	go vet ./...
	test -z "$$(gofmt -l cmd internal)"
captures: build
	./bin/lcars --capture captures --width 120 --height 36
perf: build
	python3 scripts/measure.py ./bin/lcars
