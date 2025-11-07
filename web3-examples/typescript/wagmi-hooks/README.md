# Wagmi Hooks Web3 Integration

Modern React hooks for Ethereum integration using Wagmi.

## Features

- ✅ Wallet connection (MetaMask, WalletConnect, etc.)
- ✅ Account and balance display
- ✅ Network detection
- ✅ ERC20 token interactions
- ✅ NFT minting
- ✅ Contract read/write operations
- ✅ TypeScript support
- ✅ Auto-refresh balances

## About Wagmi

Wagmi is a collection of React Hooks for Ethereum that provides:
- **Type Safety**: Full TypeScript support
- **Caching**: Intelligent data caching
- **Auto-refresh**: Real-time updates
- **Multi-chain**: Support for multiple networks
- **Modular**: Use only what you need

## Setup

```bash
npm install
```

## Development

```bash
npm run dev
```

## Build

```bash
npm run build
```

## Usage

### Setup Wagmi Provider

```tsx
import { WagmiConfig, createConfig } from 'wagmi'
import { mainnet, polygon } from 'wagmi/chains'

const config = createConfig({
  autoConnect: true,
  publicClient: createPublicClient({
    chain: mainnet,
    transport: http()
  }),
})

function App() {
  return (
    <WagmiConfig config={config}>
      <YourApp />
    </WagmiConfig>
  )
}
```

### Connect Wallet

```tsx
import { WalletConnect } from './WalletConnect'

function App() {
  return <WalletConnect />
}
```

### Read Contract

```tsx
const { data: balance } = useContractRead({
  address: '0x...',
  abi: ERC20_ABI,
  functionName: 'balanceOf',
  args: [userAddress],
})
```

### Write to Contract

```tsx
const { write } = useContractWrite({
  address: '0x...',
  abi: ERC20_ABI,
  functionName: 'transfer',
  args: [recipient, amount],
})

write()
```

## Key Hooks

### Account Hooks
- `useAccount()` - Get connected account
- `useBalance()` - Get ETH/token balance
- `useNetwork()` - Get current network

### Connection Hooks
- `useConnect()` - Connect wallet
- `useDisconnect()` - Disconnect wallet
- `useSwitchNetwork()` - Switch networks

### Contract Hooks
- `useContractRead()` - Read contract state
- `useContractWrite()` - Write to contract
- `usePrepareContractWrite()` - Prepare transaction

### Transaction Hooks
- `useSendTransaction()` - Send ETH
- `useWaitForTransaction()` - Wait for confirmation
- `useTransaction()` - Get transaction data

## Components Included

1. **WalletConnect** - Full wallet connection UI
2. **TokenBalance** - Display ERC20 balances
3. **ERC20Interaction** - Token transfers
4. **NFTMinter** - Mint NFTs with payment

## Example: Full DApp

```tsx
import { WalletConnect } from './WalletConnect'
import { ERC20Interaction } from './ContractInteraction'
import { useAccount } from 'wagmi'

function DApp() {
  const { address, isConnected } = useAccount()

  return (
    <div>
      <WalletConnect />

      {isConnected && address && (
        <ERC20Interaction
          contractAddress="0x..."
          userAddress={address}
        />
      )}
    </div>
  )
}
```

## Best Practices

1. **Always prepare writes** - Use `usePrepareContractWrite` before `useContractWrite`
2. **Handle loading states** - Show loading indicators during transactions
3. **Error handling** - Check for errors in hook returns
4. **Type safety** - Use TypeScript and define ABIs with `as const`
5. **Caching** - Let Wagmi handle caching, don't duplicate state

## Security

- ✅ Never store private keys in frontend
- ✅ Validate all user inputs
- ✅ Use PrepareContractWrite to estimate gas
- ✅ Handle transaction failures gracefully

## Resources

- [Wagmi Documentation](https://wagmi.sh/)
- [Viem Documentation](https://viem.sh/)
- [React Hooks](https://react.dev/reference/react)

## Stack

- React 18
- TypeScript 5
- Wagmi 1.4
- Viem 1.19
- Vite 5
