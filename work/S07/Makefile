.PHONY: build test lint captures performance clean
build:
	go build -buildvcs=false -trimpath -ldflags='-s -w -buildid=' -o bin/lcars ./cmd/lcars
test:
	go test -race ./...
lint:
	go vet ./...
	test -z "$$(gofmt -l cmd internal)"
captures:
	go run -buildvcs=false ./cmd/capture -out artifacts
performance: build
	python3 scripts/verify_terminal.py --binary bin/lcars --seconds 12 --output artifacts/performance.json
clean:
	go clean ./...
