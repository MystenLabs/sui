# Makefile for Sui repository common tasks

.PHONY: check build test simtest lint fmt clippy xlint clean help

# Default target
help:
	@echo "Sui Development Commands"
	@echo "========================"
	@echo ""
	@echo "Build:"
	@echo "  make check          - Check code without building (fast)"
	@echo "  make build          - Build the entire project"
	@echo "  make build-p P=pkg  - Build a specific package (e.g., make build-p P=sui-core)"
	@echo ""
	@echo "Test:"
	@echo "  make test           - Run unit tests (skips simtests)"
	@echo "  make test-p P=pkg   - Run tests for specific package(s)"
	@echo "  make test-lib       - Run only library tests (faster)"
	@echo "  make simtest        - Run simulation tests"
	@echo "  make simtest-p P=pkg- Run simtests for specific package"
	@echo ""
	@echo "Lint:"
	@echo "  make lint           - Run all formatting and linting (recommended before commit)"
	@echo "  make fmt            - Format Rust code"
	@echo "  make fmt-check      - Check Rust formatting without changes"
	@echo "  make clippy         - Run clippy lints"
	@echo "  make xlint          - Run xlint"
	@echo ""
	@echo "Other:"
	@echo "  make clean          - Clean build artifacts"
	@echo ""

# Build targets
check:
	cargo check

build:
	cargo build

build-p:
	@if [ -z "$(P)" ]; then echo "Usage: make build-p P=<package>"; exit 1; fi
	cargo build -p $(P)

# Test targets
test:
	SUI_SKIP_SIMTESTS=1 cargo nextest run

test-p:
	@if [ -z "$(P)" ]; then echo "Usage: make test-p P=<package>"; exit 1; fi
	SUI_SKIP_SIMTESTS=1 cargo nextest run -p $(P)

test-lib:
	SUI_SKIP_SIMTESTS=1 cargo nextest run --lib

simtest:
	cargo simtest

simtest-p:
	@if [ -z "$(P)" ]; then echo "Usage: make simtest-p P=<package>"; exit 1; fi
	cargo simtest -p $(P)

# Lint targets
lint:
	./scripts/lint.sh

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo xclippy -D warnings

xlint:
	cargo xlint

# Clean
clean:
	cargo clean
