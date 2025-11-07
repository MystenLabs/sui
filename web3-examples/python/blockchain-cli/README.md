# Blockchain CLI

Command-line interface for Ethereum blockchain operations.

## Features

- ✅ Check address balances
- ✅ View block information
- ✅ Query transactions and receipts
- ✅ Create new wallets
- ✅ Check gas prices
- ✅ Network information
- ✅ Easy-to-use CLI

## Installation

```bash
pip install -r requirements.txt
chmod +x cli.py
```

## Usage

### Check Balance

```bash
./cli.py balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045
```

Output:
```
✅ Connected to blockchain (Chain ID: 1)

💰 Balance: 123.456 ETH
   Address: 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045
```

### View Block

```bash
# Latest block
./cli.py block

# Specific block
./cli.py block 18000000
```

Output:
```
🔷 Block #18000000
   Hash: 0x1234...
   Timestamp: 1693838400
   Transactions: 150
   Gas Used: 15,000,000
   Miner: 0xabcd...
```

### Transaction Details

```bash
./cli.py tx 0x1234567890abcdef...
```

### Transaction Receipt

```bash
./cli.py receipt 0x1234567890abcdef...
```

Output:
```
📋 Transaction Receipt
   Status: ✅ Success
   Block: 18000000
   Gas Used: 21,000
   Logs: 0
```

### Create Wallet

```bash
./cli.py create-wallet
```

Output:
```
🔐 New Wallet Created
   Address: 0x1234...
   Private Key: 0xabcd...

⚠️  WARNING: Save your private key securely!
   Never share it or commit it to version control!
```

### Gas Price

```bash
./cli.py gas-price
```

Output:
```
⛽ Current Gas Price
   25.5 Gwei
   25,500,000,000 Wei
```

### Network Info

```bash
./cli.py network-info
```

Output:
```
🌐 Network Information
   Chain ID: 1
   Latest Block: 18,500,000
   Gas Price: 25.5 Gwei
   Syncing: False
```

## Custom RPC

Use `--rpc` flag to specify custom endpoint:

```bash
./cli.py --rpc https://polygon-rpc.com balance 0x...
```

## Examples

```bash
# Check Vitalik's balance
./cli.py balance 0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045

# View latest block
./cli.py block

# Check gas price on Polygon
./cli.py --rpc https://polygon-rpc.com gas-price

# Create new wallet
./cli.py create-wallet
```

## Commands

| Command | Description |
|---------|-------------|
| `balance ADDRESS` | Get ETH balance |
| `block [NUMBER]` | Get block info |
| `tx HASH` | Get transaction |
| `receipt HASH` | Get receipt |
| `create-wallet` | Create wallet |
| `gas-price` | Current gas price |
| `network-info` | Network details |

## Help

```bash
./cli.py --help
./cli.py balance --help
```

## Dependencies

- `web3` >= 6.11.0
- `eth-account` >= 0.10.0
- `colorama` >= 0.4.6

## Tips

- Use environment variables for RPC URLs
- Save output with `> output.txt`
- Pipe to other commands with `|`
- Use `watch` for monitoring: `watch -n 5 ./cli.py gas-price`

## Resources

- [Web3.py Documentation](https://web3py.readthedocs.io/)
- [Ethereum JSON-RPC](https://ethereum.org/en/developers/docs/apis/json-rpc/)
