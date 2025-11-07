# Node Setup Scripts

Automated setup scripts for blockchain nodes.

## Features

- ✅ Geth installation
- ✅ Erigon installation
- ✅ Systemd service setup
- ✅ Multi-network support
- ✅ HTTP + WebSocket RPC
- ✅ Auto-restart on failure

## Usage

### Install and Start Geth

```bash
./setup-node.sh
```

### Specific Network

```bash
NETWORK=goerli NODE_TYPE=geth ./setup-node.sh
```

### Custom Ports

```bash
HTTP_PORT=9545 WS_PORT=9546 ./setup-node.sh
```

### Install Erigon

```bash
NODE_TYPE=erigon ./setup-node.sh
```

## Environment Variables

- `NODE_TYPE` - Node implementation (geth, erigon)
- `NETWORK` - Network to sync (mainnet, goerli, sepolia)
- `DATA_DIR` - Blockchain data directory
- `HTTP_PORT` - HTTP RPC port (default: 8545)
- `WS_PORT` - WebSocket port (default: 8546)

## Supported Nodes

### Geth (Go Ethereum)
- Official Ethereum implementation
- Fast sync modes
- Full and light clients

### Erigon
- Performance-optimized
- Lower disk usage
- Faster sync times

## Node Management

### Check Status
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

### Check Sync Progress
```bash
geth attach http://localhost:8545 --exec eth.syncing
```

### View Logs
```bash
journalctl -u geth-node -f
```

### Stop Node
```bash
sudo systemctl stop geth-node
```

### Start Node
```bash
sudo systemctl start geth-node
```

## Requirements

- Ubuntu 20.04+ or macOS
- 500GB+ free disk space
- 8GB+ RAM recommended
- Stable internet connection

## Hardware Recommendations

### Mainnet Full Node
- CPU: 4+ cores
- RAM: 16GB+
- Disk: 1TB SSD
- Network: 100Mbps+

### Testnet Node
- CPU: 2+ cores
- RAM: 8GB+
- Disk: 100GB SSD

## Security

- Configure firewall rules
- Use reverse proxy for public access
- Enable authentication for RPC
- Keep node software updated
