# Vyper Simple Vault

A secure vault contract written in Vyper for depositing and withdrawing ETH.

## Features

- ✅ Deposit ETH to vault
- ✅ Withdraw specific amounts
- ✅ Withdraw all deposited funds
- ✅ Track individual balances
- ✅ Event logging for all operations
- ✅ Vyper's built-in security features

## About Vyper

Vyper is a pythonic smart contract language for the EVM that emphasizes:
- **Security**: Auditable, minimalistic syntax
- **Simplicity**: Easy to understand and reason about
- **Auditability**: No hidden surprises or complex features

## Setup

Install Vyper:
```bash
pip install vyper
```

## Compile

```bash
vyper Vault.vy
```

Compile with ABI:
```bash
vyper -f abi Vault.vy
```

## Test

```bash
# Using Brownie
brownie test

# Using Ape
ape test
```

## Deploy

```bash
# Using Brownie
brownie run scripts/deploy.py

# Using Ape
ape run deploy
```

## Contract Interface

### Write Functions

- `deposit()` - Deposit ETH (payable)
- `withdraw(amount)` - Withdraw specific amount
- `withdraw_all()` - Withdraw entire balance

### Read Functions

- `get_balance(account)` - Get account balance
- `get_total_supply()` - Get total vault balance
- `balances(address)` - Direct balance lookup
- `total_supply()` - Direct total supply lookup

## Security Features

- ✅ Overflow protection (built-in)
- ✅ Reentrancy protection (safe send)
- ✅ Clear access control
- ✅ Minimal attack surface

## Why Vyper?

- Pythonic syntax
- Security-focused design
- No modifiers or class inheritance
- Bounds and overflow checking
- Clear and explicit code

## Dependencies

- Vyper ^0.3.7
- Python 3.8+

## Resources

- [Vyper Documentation](https://docs.vyperlang.org/)
- [Vyper GitHub](https://github.com/vyperlang/vyper)
