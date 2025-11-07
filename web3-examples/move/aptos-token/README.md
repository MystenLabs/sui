# Aptos Simple Coin

A fungible token (coin) implementation on Aptos blockchain using Move.

## Features

- ✅ Initialize custom coin with metadata
- ✅ Mint new tokens (admin only)
- ✅ Burn tokens
- ✅ Transfer between accounts
- ✅ Query balance and total supply
- ✅ Built on Aptos Framework coin standard
- ✅ Comprehensive unit tests

## About Aptos

Aptos is a Layer 1 blockchain featuring:
- **Performance**: 160,000+ TPS capability
- **Safety**: Move language with formal verification
- **Scalability**: Parallel execution engine (Block-STM)
- **User Experience**: Account abstraction support

## Setup

Install Aptos CLI:
```bash
curl -fsSL "https://aptos.dev/scripts/install_cli.py" | python3
```

## Build

```bash
aptos move compile
```

## Test

```bash
aptos move test
```

## Deploy

```bash
# Initialize account (if needed)
aptos init

# Fund account with test tokens (devnet)
aptos account fund-with-faucet --account default

# Publish module
aptos move publish --named-addresses aptos_token=default
```

## Usage

### Initialize Coin

```bash
aptos move run \
  --function-id 'YOUR_ADDRESS::simple_coin::initialize'
```

### Register to Receive Tokens

```bash
aptos move run \
  --function-id 'MODULE_ADDRESS::simple_coin::register'
```

### Mint Tokens

```bash
aptos move run \
  --function-id 'MODULE_ADDRESS::simple_coin::mint' \
  --args address:RECIPIENT_ADDRESS u64:1000000
```

### Transfer Tokens

```bash
aptos move run \
  --function-id 'MODULE_ADDRESS::simple_coin::transfer' \
  --args address:RECIPIENT_ADDRESS u64:500000
```

### Burn Tokens

```bash
aptos move run \
  --function-id 'MODULE_ADDRESS::simple_coin::burn' \
  --args u64:100000
```

### Query Balance

```bash
aptos move view \
  --function-id 'MODULE_ADDRESS::simple_coin::balance' \
  --args address:ACCOUNT_ADDRESS
```

### Query Total Supply

```bash
aptos move view \
  --function-id 'MODULE_ADDRESS::simple_coin::total_supply'
```

## Module Structure

```move
module aptos_token::simple_coin {
    struct SimpleCoin { }  // Coin type

    struct Capabilities {  // Admin capabilities
        mint_cap,
        burn_cap,
        freeze_cap
    }

    // Functions
    - initialize()
    - register()
    - mint()
    - burn()
    - transfer()
    - balance()
    - total_supply()
}
```

## Token Details

- **Name**: Simple Coin
- **Symbol**: SMPL
- **Decimals**: 8
- **Supply Monitoring**: Enabled

## Security Features

- ✅ Admin-only minting
- ✅ Capability-based access control
- ✅ Move's resource safety
- ✅ Formal verification ready

## Testing

Run the comprehensive test suite:
```bash
aptos move test --coverage
```

Tests include:
- Minting and transferring
- Burning mechanics
- Balance tracking
- Supply management

## Architecture

```
┌─────────────────┐
│  Admin Account  │
│ (Capabilities)  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  SimpleCoin     │
│   Module        │
├─────────────────┤
│ • Mint          │
│ • Burn          │
│ • Transfer      │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ User Accounts   │
│  (Balances)     │
└─────────────────┘
```

## Best Practices

1. **Initialization**: Call `initialize()` only once by module owner
2. **Registration**: Users must call `register()` before receiving tokens
3. **Capabilities**: Store mint/burn capabilities securely
4. **Testing**: Always test on devnet before mainnet

## Resources

- [Aptos Documentation](https://aptos.dev/)
- [Move on Aptos](https://aptos.dev/move/move-on-aptos/)
- [Aptos Token Standard](https://aptos.dev/standards/aptos-token/)
- [Aptos Explorer](https://explorer.aptoslabs.com/)
