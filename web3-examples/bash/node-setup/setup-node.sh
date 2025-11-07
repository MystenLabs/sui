#!/bin/bash

###############################################################################
# Blockchain Node Setup Script
# Automated setup for Ethereum/EVM nodes
###############################################################################

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Configuration
NODE_TYPE="${NODE_TYPE:-geth}"
DATA_DIR="${DATA_DIR:-./blockchain-data}"
NETWORK="${NETWORK:-mainnet}"
HTTP_PORT="${HTTP_PORT:-8545}"
WS_PORT="${WS_PORT:-8546}"

install_geth() {
    log_info "Installing Geth (Go Ethereum)..."

    if command -v geth &> /dev/null; then
        log_success "Geth already installed: $(geth version | head -n 1)"
        return
    fi

    # Install based on OS
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo add-apt-repository -y ppa:ethereum/ethereum
        sudo apt-get update
        sudo apt-get install -y ethereum
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        brew tap ethereum/ethereum
        brew install ethereum
    else
        log_error "Unsupported OS: $OSTYPE"
        exit 1
    fi

    log_success "Geth installed successfully"
}

install_erigon() {
    log_info "Installing Erigon..."

    if command -v erigon &> /dev/null; then
        log_success "Erigon already installed"
        return
    fi

    # Clone and build
    git clone https://github.com/ledgerwatch/erigon.git
    cd erigon
    make erigon
    sudo cp build/bin/erigon /usr/local/bin/
    cd ..
    rm -rf erigon

    log_success "Erigon installed successfully"
}

setup_systemd_service() {
    local service_name="$1"
    local exec_command="$2"

    log_info "Setting up systemd service: $service_name..."

    sudo tee /etc/systemd/system/$service_name.service > /dev/null <<EOF
[Unit]
Description=Ethereum Node ($service_name)
After=network.target

[Service]
Type=simple
User=$USER
ExecStart=$exec_command
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

    sudo systemctl daemon-reload
    sudo systemctl enable $service_name
    log_success "Systemd service created"
}

start_geth_node() {
    log_info "Starting Geth node..."

    local cmd="geth \
        --datadir $DATA_DIR \
        --http \
        --http.addr 0.0.0.0 \
        --http.port $HTTP_PORT \
        --http.api eth,net,web3,txpool \
        --ws \
        --ws.addr 0.0.0.0 \
        --ws.port $WS_PORT \
        --ws.api eth,net,web3 \
        --syncmode snap \
        --cache 2048"

    if [ "$NETWORK" = "goerli" ]; then
        cmd="$cmd --goerli"
    elif [ "$NETWORK" = "sepolia" ]; then
        cmd="$cmd --sepolia"
    fi

    $cmd &

    log_success "Geth node started"
    log_info "HTTP RPC: http://localhost:$HTTP_PORT"
    log_info "WebSocket: ws://localhost:$WS_PORT"
}

check_node_status() {
    log_info "Checking node status..."

    sleep 5

    if curl -s -X POST http://localhost:$HTTP_PORT \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | grep -q "result"; then
        log_success "Node is running and responding"
    else
        log_error "Node is not responding"
        exit 1
    fi
}

display_info() {
    echo ""
    echo "====================================="
    echo "  Node Setup Complete"
    echo "====================================="
    echo ""
    echo "Node Type: $NODE_TYPE"
    echo "Network: $NETWORK"
    echo "Data Directory: $DATA_DIR"
    echo "HTTP RPC: http://localhost:$HTTP_PORT"
    echo "WebSocket: ws://localhost:$WS_PORT"
    echo ""
    echo "Useful commands:"
    echo "  - Check sync status: geth attach http://localhost:$HTTP_PORT --exec eth.syncing"
    echo "  - View logs: journalctl -u geth-node -f"
    echo "  - Stop node: sudo systemctl stop geth-node"
    echo ""
}

main() {
    log_info "Starting blockchain node setup..."
    log_info "Node type: $NODE_TYPE"
    log_info "Network: $NETWORK"
    echo ""

    case $NODE_TYPE in
        geth)
            install_geth
            start_geth_node
            ;;
        erigon)
            install_erigon
            log_info "Start Erigon manually with your preferred configuration"
            ;;
        *)
            log_error "Unknown node type: $NODE_TYPE"
            exit 1
            ;;
    esac

    check_node_status
    display_info

    log_success "Setup completed successfully! 🎉"
}

main "$@"
