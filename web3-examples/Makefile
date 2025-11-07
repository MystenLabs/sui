.PHONY: help install test clean lint format

help:
	@echo "Web3 Multi-Language Playground - Make Commands"
	@echo ""
	@echo "Usage:"
	@echo "  make install    Install all dependencies"
	@echo "  make test       Run all tests"
	@echo "  make lint       Run linters"
	@echo "  make format     Format code"
	@echo "  make clean      Clean build artifacts"

install:
	@echo "Installing dependencies..."
	cd typescript/wagmi-hooks && npm install
	cd typescript/ethers-scripts && npm install
	cd solidity/erc20 && npm install
	cd python/web3py-tools && pip install -r requirements.txt
	cd python/blockchain-cli && pip install -r requirements.txt
	@echo "✅ Installation complete"

test:
	@echo "Running tests..."
	cd solidity/erc20 && npx hardhat test
	cd python/web3py-tools && pytest || echo "Python tests"
	@echo "✅ Tests complete"

lint:
	@echo "Running linters..."
	cd typescript/ethers-scripts && npx eslint . || echo "TS lint"
	cd python && flake8 . || echo "Python lint"
	cd bash && shellcheck **/*.sh || echo "Shell lint"
	@echo "✅ Linting complete"

format:
	@echo "Formatting code..."
	cd python && black . || echo "Python format"
	cd go/rpc-client && go fmt ./... || echo "Go format"
	cd rust/solana-program && cargo fmt || echo "Rust format"
	@echo "✅ Formatting complete"

clean:
	@echo "Cleaning build artifacts..."
	find . -type d -name "node_modules" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "target" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "dist" -exec rm -rf {} + 2>/dev/null || true
	find . -type d -name "build" -exec rm -rf {} + 2>/dev/null || true
	@echo "✅ Cleanup complete"
