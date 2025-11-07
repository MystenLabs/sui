# Python Web3.py Tools

Comprehensive blockchain interaction toolkit using Web3.py

## Features

- ✅ Blockchain client for ETH operations
- ✅ ERC20 token interactions
- ✅ Account creation and management
- ✅ Transaction sending and monitoring
- ✅ Balance queries
- ✅ Block and transaction lookups
- ✅ Type hints and documentation

## Setup

```bash
pip install -r requirements.txt
```

## Usage

### Blockchain Client

```python
from blockchain_client import BlockchainClient

# Connect to network
client = BlockchainClient('https://eth.llamarpc.com')

# Get balance
balance = client.get_balance('0x...')
print(f"Balance: {balance} ETH")

# Get latest block
block = client.get_block()
print(f"Block: {block['number']}")

# Send transaction
tx_hash = client.send_transaction(
    private_key='0x...',
    to_address='0x...',
    value_eth=0.1
)

# Wait for confirmation
receipt = client.wait_for_transaction(tx_hash)
```

### ERC20 Token Operations

```python
from blockchain_client import ERC20Client
from web3 import Web3

w3 = Web3(Web3.HTTPProvider('https://eth.llamarpc.com'))
token = ERC20Client(w3, '0xTokenAddress...')

# Get token info
info = token.get_info()
print(info)  # {'name': 'Token', 'symbol': 'TKN', ...}

# Check balance
balance = token.balance_of('0x...')

# Transfer tokens
tx_hash = token.transfer(
    private_key='0x...',
    to_address='0x...',
    amount=100.0
)
```

## Classes

### BlockchainClient

Main client for blockchain interactions:
- `get_balance(address)` - Get ETH balance
- `get_block(number)` - Get block information
- `get_transaction(tx_hash)` - Get transaction details
- `send_transaction(...)` - Send ETH transaction
- `wait_for_transaction(tx_hash)` - Wait for confirmation

### ERC20Client

ERC20 token interactions:
- `get_info()` - Token metadata
- `balance_of(address)` - Token balance
- `transfer(...)` - Transfer tokens

## Examples

### Create New Account

```python
from eth_account import Account

account = Account.create()
print(f"Address: {account.address}")
print(f"Private key: {account.key.hex()}")
```

### Monitor Transactions

```python
import time

while True:
    block = client.get_block()
    print(f"Block {block['number']}: {len(block['transactions'])} txs")
    time.sleep(12)
```

## Security

- Never commit private keys
- Use environment variables for sensitive data
- Validate all addresses before use
- Test on testnet first

## Dependencies

- `web3` >= 6.11.0
- `eth-account` >= 0.10.0

## Resources

- [Web3.py Documentation](https://web3py.readthedocs.io/)
- [Ethereum Python](https://ethereum.org/en/developers/docs/programming-languages/python/)
