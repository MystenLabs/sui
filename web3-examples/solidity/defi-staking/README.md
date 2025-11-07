# DeFi Staking Contract

A production-ready staking contract with reward distribution mechanism.

## Features

- ✅ Stake ERC20 tokens
- ✅ Earn rewards over time
- ✅ Withdraw staked tokens
- ✅ Claim accumulated rewards
- ✅ ReentrancyGuard protection
- ✅ Configurable reward rate

## How It Works

1. Users stake their tokens
2. Rewards accrue based on time and staking amount
3. Users can withdraw their stake at any time
4. Users can claim their earned rewards

## Setup

```bash
npm install
```

## Compile

```bash
npx hardhat compile
```

## Usage

```javascript
// Approve staking contract
await stakingToken.approve(stakingContract.address, amount);

// Stake tokens
await stakingContract.stake(ethers.parseEther("100"));

// Check earned rewards
const earned = await stakingContract.earned(userAddress);

// Claim rewards
await stakingContract.getReward();

// Withdraw stake
await stakingContract.withdraw(ethers.parseEther("50"));

// Exit (withdraw all + claim rewards)
await stakingContract.exit();
```

## Key Functions

- `stake(amount)` - Stake tokens
- `withdraw(amount)` - Withdraw staked tokens
- `getReward()` - Claim earned rewards
- `earned(account)` - View pending rewards
- `exit()` - Withdraw all + claim rewards

## Security Features

- ✅ ReentrancyGuard
- ✅ OpenZeppelin security patterns
- ✅ Owner-only admin functions
- ✅ Safe math operations

## Dependencies

- Solidity ^0.8.20
- OpenZeppelin Contracts ^5.0.0
