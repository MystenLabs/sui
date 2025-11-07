# NEAR Guestbook Contract

A decentralized guestbook smart contract built on NEAR Protocol with donation support.

## Features

- ✅ Add messages to guestbook
- ✅ Optional donations with messages
- ✅ View message history
- ✅ Get recent messages
- ✅ Track total donations
- ✅ Owner withdrawal mechanism

## About NEAR Protocol

NEAR is a user-friendly blockchain featuring:
- **Fast**: 1-2 second finality
- **Cheap**: Fraction of a cent per transaction
- **Scalable**: Sharding for horizontal scaling
- **Human-Readable Accounts**: alice.near instead of 0x...

## Setup

Install NEAR CLI:
```bash
npm install -g near-cli
```

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

## Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

Or using cargo-near:
```bash
cargo install cargo-near
cargo near build
```

## Test

```bash
cargo test
```

## Deploy

```bash
# Login to NEAR
near login

# Create sub-account for contract
near create-account guestbook.YOUR_ACCOUNT.testnet --masterAccount YOUR_ACCOUNT.testnet

# Deploy contract
near deploy --accountId guestbook.YOUR_ACCOUNT.testnet --wasmFile target/wasm32-unknown-unknown/release/near_guestbook.wasm
```

## Usage

### Add Message

```bash
near call guestbook.YOUR_ACCOUNT.testnet add_message '{"text": "Hello NEAR!"}' --accountId YOUR_ACCOUNT.testnet
```

### Add Message with Donation

```bash
near call guestbook.YOUR_ACCOUNT.testnet add_message '{"text": "Love NEAR!"}' --accountId YOUR_ACCOUNT.testnet --deposit 1
```

### Get Message Count

```bash
near view guestbook.YOUR_ACCOUNT.testnet get_message_count
```

### Get Recent Messages

```bash
near view guestbook.YOUR_ACCOUNT.testnet get_recent_messages '{"count": 5}'
```

### Get Specific Message

```bash
near view guestbook.YOUR_ACCOUNT.testnet get_message '{"id": 0}'
```

### Get Total Donations

```bash
near view guestbook.YOUR_ACCOUNT.testnet get_total_donations
```

## Contract Methods

### Write Methods (change state)

- `add_message(text: String)` - Add message (payable)

### View Methods (read-only)

- `get_message_count()` - Get total messages
- `get_message(id: u64)` - Get message by ID
- `get_recent_messages(count: u64)` - Get last N messages
- `get_total_donations()` - Get total NEAR donated

### Owner Methods

- `withdraw(amount: Balance)` - Withdraw donations

## Architecture

```
┌──────────────────┐
│   User Wallet    │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ NEAR Guestbook   │
│   Contract       │
├──────────────────┤
│ • Messages       │
│ • Donations      │
│ • Timestamps     │
└──────────────────┘
```

## Security Features

- ✅ Message length validation
- ✅ Owner-only withdrawal
- ✅ Overflow protection
- ✅ NEAR SDK security patterns

## Gas Optimization

The contract is optimized for:
- Minimal storage usage
- Efficient querying
- Low gas costs

## Dependencies

- `near-sdk`: ^4.1.1

## Resources

- [NEAR Documentation](https://docs.near.org/)
- [NEAR SDK Rust](https://www.near-sdk.io/)
- [NEAR Examples](https://examples.near.org/)
