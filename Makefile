# Makefile for Sui repository common tasks

.DEFAULT_GOAL := help

##@ Help

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Build

.PHONY: check
check: ## Check code without building (fast).
	cargo check

.PHONY: build
build: ## Build the entire project.
	cargo build

.PHONY: build-p
build-p: ## Build a specific package (P=<package>).
	@if [ -z "$(P)" ]; then echo "Usage: make build-p P=<package>"; exit 1; fi
	cargo build -p $(P)

##@ Test

.PHONY: test
test: ## Run unit tests (skips simtests).
	SUI_SKIP_SIMTESTS=1 cargo nextest run

.PHONY: test-p
test-p: ## Run tests for specific package (P=<package>).
	@if [ -z "$(P)" ]; then echo "Usage: make test-p P=<package>"; exit 1; fi
	SUI_SKIP_SIMTESTS=1 cargo nextest run -p $(P)

.PHONY: test-lib
test-lib: ## Run only library tests (faster).
	SUI_SKIP_SIMTESTS=1 cargo nextest run --lib

.PHONY: simtest
simtest: ## Run simulation tests.
	cargo simtest

.PHONY: simtest-p
simtest-p: ## Run simtests for specific package (P=<package>).
	@if [ -z "$(P)" ]; then echo "Usage: make simtest-p P=<package>"; exit 1; fi
	cargo simtest -p $(P)

##@ Lint

.PHONY: lint
lint: ## Run all formatting and linting (recommended before commit).
	./scripts/lint.sh

.PHONY: fmt
fmt: ## Format Rust code.
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Check Rust formatting without changes.
	cargo fmt --all -- --check

.PHONY: clippy
clippy: ## Run clippy lints with warnings as errors.
	cargo xclippy -D warnings

.PHONY: xlint
xlint: ## Run xlint.
	cargo xlint

##@ Other

.PHONY: clean
clean: ## Clean build artifacts.
	cargo clean
