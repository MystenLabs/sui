#!/bin/bash

###############################################################################
# Smart Contract Deployment Script
# Automated deployment for Ethereum smart contracts
###############################################################################

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
NETWORK="${NETWORK:-localhost}"
GAS_PRICE="${GAS_PRICE:-auto}"
GAS_LIMIT="${GAS_LIMIT:-5000000}"
CONTRACTS_DIR="./contracts"
BUILD_DIR="./build"

# Functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_dependencies() {
    log_info "Checking dependencies..."

    if ! command -v node &> /dev/null; then
        log_error "Node.js is not installed"
        exit 1
    fi

    if ! command -v npx &> /dev/null; then
        log_error "npx is not installed"
        exit 1
    fi

    if [ ! -f "hardhat.config.js" ] && [ ! -f "hardhat.config.ts" ]; then
        log_warning "Hardhat config not found. Is this a Hardhat project?"
    fi

    log_success "All dependencies found"
}

compile_contracts() {
    log_info "Compiling smart contracts..."

    if [ -f "hardhat.config.js" ] || [ -f "hardhat.config.ts" ]; then
        npx hardhat compile
    elif [ -f "foundry.toml" ]; then
        forge build
    else
        log_error "No supported build tool configuration found"
        exit 1
    fi

    log_success "Contracts compiled successfully"
}

run_tests() {
    log_info "Running tests..."

    if [ -f "hardhat.config.js" ] || [ -f "hardhat.config.ts" ]; then
        npx hardhat test
    elif [ -f "foundry.toml" ]; then
        forge test
    fi

    log_success "All tests passed"
}

deploy_contracts() {
    log_info "Deploying contracts to $NETWORK..."

    if [ -f "hardhat.config.js" ] || [ -f "hardhat.config.ts" ]; then
        npx hardhat run scripts/deploy.js --network "$NETWORK"
    elif [ -f "foundry.toml" ]; then
        forge script scripts/Deploy.s.sol --rpc-url "$NETWORK" --broadcast
    fi

    log_success "Contracts deployed successfully"
}

verify_contracts() {
    local contract_address=$1
    local contract_name=$2

    log_info "Verifying contract $contract_name at $contract_address..."

    if [ -f "hardhat.config.js" ] || [ -f "hardhat.config.ts" ]; then
        npx hardhat verify --network "$NETWORK" "$contract_address"
    elif [ -f "foundry.toml" ]; then
        forge verify-contract "$contract_address" "$contract_name" --chain-id "$NETWORK"
    fi

    log_success "Contract verified"
}

save_deployment_info() {
    local deployment_file="deployments/${NETWORK}-deployment.json"

    log_info "Saving deployment information to $deployment_file..."

    mkdir -p deployments

    cat > "$deployment_file" <<EOF
{
  "network": "$NETWORK",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "gasPrice": "$GAS_PRICE",
  "gasLimit": "$GAS_LIMIT",
  "deployer": "$(cast wallet address 2>/dev/null || echo 'N/A')",
  "contracts": {}
}
EOF

    log_success "Deployment info saved"
}

main() {
    echo ""
    log_info "====================================="
    log_info "  Smart Contract Deployment Script  "
    log_info "====================================="
    echo ""

    log_info "Network: $NETWORK"
    log_info "Gas Price: $GAS_PRICE"
    log_info "Gas Limit: $GAS_LIMIT"
    echo ""

    # Parse command line arguments
    SKIP_TESTS=false
    SKIP_VERIFY=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --skip-verify)
                SKIP_VERIFY=true
                shift
                ;;
            --network)
                NETWORK="$2"
                shift 2
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Run deployment steps
    check_dependencies
    compile_contracts

    if [ "$SKIP_TESTS" = false ]; then
        run_tests
    else
        log_warning "Skipping tests"
    fi

    deploy_contracts
    save_deployment_info

    echo ""
    log_success "Deployment completed successfully! 🎉"
    echo ""
}

# Run main function
main "$@"
