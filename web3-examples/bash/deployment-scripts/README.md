# Deployment Scripts

Automated deployment scripts for blockchain smart contracts.

## Features

- ✅ Contract compilation
- ✅ Automated testing
- ✅ Multi-network deployment
- ✅ Contract verification
- ✅ Deployment tracking
- ✅ Error handling
- ✅ Colored output

## Usage

### Basic Deployment

```bash
./deploy-contracts.sh
```

### Deploy to Specific Network

```bash
NETWORK=goerli ./deploy-contracts.sh
```

### Skip Tests

```bash
./deploy-contracts.sh --skip-tests
```

### Skip Verification

```bash
./deploy-contracts.sh --skip-verify
```

### Custom Gas Settings

```bash
GAS_PRICE=50 GAS_LIMIT=8000000 ./deploy-contracts.sh --network mainnet
```

## Environment Variables

- `NETWORK` - Target network (default: localhost)
- `GAS_PRICE` - Gas price in Gwei (default: auto)
- `GAS_LIMIT` - Gas limit (default: 5000000)

## Supported Networks

- localhost
- goerli
- sepolia
- mainnet
- polygon
- arbitrum
- optimism

## Output

The script will:
1. Check dependencies
2. Compile contracts
3. Run tests (optional)
4. Deploy contracts
5. Verify contracts (optional)
6. Save deployment info

## Deployment Info

Deployment details are saved to:
```
deployments/{network}-deployment.json
```

## Requirements

- Node.js
- Hardhat or Foundry
- Network RPC access
- Deployer wallet with funds
