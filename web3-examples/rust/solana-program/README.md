# Solana Counter Program

A simple on-chain counter program demonstrating Solana program development in Rust.

## Features

- ✅ Initialize counter
- ✅ Increment counter
- ✅ Decrement counter
- ✅ Borsh serialization
- ✅ Unit tests included
- ✅ Safe arithmetic operations

## About Solana

Solana is a high-performance blockchain featuring:
- **Speed**: 65,000+ TPS
- **Low Cost**: Fraction of a penny per transaction
- **Rust-Native**: Programs written in Rust
- **Account Model**: Unique data storage approach

## Setup

Install Solana CLI:
```bash
sh -c "$(curl -sSfL https://release.solana.com/v1.17.0/install)"
```

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Build

```bash
cargo build-bpf
```

## Test

```bash
cargo test
```

## Deploy

```bash
# Set cluster
solana config set --url devnet

# Create keypair (if needed)
solana-keygen new

# Airdrop SOL for deployment
solana airdrop 2

# Deploy
solana program deploy target/deploy/solana_counter_program.so
```

## Usage

```typescript
// Initialize counter
await program.methods
  .initialize()
  .accounts({ counter: counterPda })
  .rpc();

// Increment
await program.methods
  .increment()
  .accounts({ counter: counterPda })
  .rpc();

// Decrement
await program.methods
  .decrement()
  .accounts({ counter: counterPda })
  .rpc();
```

## Instruction Format

- `0` - Initialize counter to 0
- `1` - Increment counter by 1
- `2` - Decrement counter by 1

## Program Architecture

```
┌─────────────────┐
│   Client App    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Solana Program  │
│  (On-chain)     │
├─────────────────┤
│ • Initialize    │
│ • Increment     │
│ • Decrement     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Counter Account │
│   (State)       │
└─────────────────┘
```

## Security Features

- ✅ Owner verification
- ✅ Checked arithmetic
- ✅ Borsh serialization safety
- ✅ Program-owned accounts only

## Dependencies

- `solana-program`: ^1.17
- `borsh`: ^0.10

## Resources

- [Solana Documentation](https://docs.solana.com/)
- [Solana Cookbook](https://solanacookbook.com/)
- [Anchor Framework](https://www.anchor-lang.com/)
