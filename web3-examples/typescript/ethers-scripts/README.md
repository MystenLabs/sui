# Ethers.js Scripts

Complete Web3 operations toolkit using ethers.js library.

## Features

- ✅ Wallet creation and import
- ✅ Balance queries
- ✅ ETH transfers
- ✅ Message signing and verification
- ✅ ERC20 token operations
- ✅ Event listening
- ✅ Multiple provider support
- ✅ TypeScript support

## About Ethers.js

Ethers.js is a complete Ethereum library featuring:
- **Compact**: Small bundle size
- **Complete**: Full Ethereum functionality
- **Well-tested**: Extensive test coverage
- **TypeScript**: First-class TypeScript support
- **MIT Licensed**: Free for any use

## Setup

```bash
npm install
```

## Build

```bash
npm run build
```

## Run

```bash
npm start
```

## Usage Examples

### Create Wallet

```typescript
import { createWallet } from './wallet-operations';

const wallet = createWallet();
console.log('Address:', wallet.address);
```

### Get Balance

```typescript
import { getBalance, getProvider } from './wallet-operations';

const provider = getProvider();
const balance = await getBalance('0x...', provider);
console.log(`Balance: ${balance} ETH`);
```

### Send ETH

```typescript
import { sendEther, importWallet, getProvider } from './wallet-operations';

const provider = getProvider();
const wallet = importWallet('PRIVATE_KEY', provider);

await sendEther(wallet, '0xRecipient...', '0.1');
```

### Sign Message

```typescript
import { signMessage, verifySignature } from './wallet-operations';

const signature = await signMessage(wallet, 'Hello Web3!');
const recovered = verifySignature('Hello Web3!', signature);
```

### ERC20 Operations

```typescript
import { ERC20Token } from './wallet-operations';

const token = new ERC20Token('0xTokenAddress...', wallet);

// Get token info
const info = await token.getInfo();
console.log(info); // { name, symbol, decimals, totalSupply }

// Check balance
const balance = await token.balanceOf('0x...');

// Transfer tokens
await token.transfer('0xRecipient...', '100');

// Approve spending
await token.approve('0xSpender...', '1000');
```

### Listen to Events

```typescript
import { listenToTransfers } from './wallet-operations';

await listenToTransfers('0xTokenAddress...', provider);
// Logs all Transfer events in real-time
```

## Provider Options

### JSON-RPC Provider

```typescript
const provider = new ethers.JsonRpcProvider('https://eth.llamarpc.com');
```

### Browser Provider (MetaMask)

```typescript
const provider = new ethers.BrowserProvider(window.ethereum);
```

### Default Provider

```typescript
const provider = ethers.getDefaultProvider('mainnet');
```

### WebSocket Provider

```typescript
const provider = new ethers.WebSocketProvider('wss://...');
```

## Common Operations

### Format/Parse Values

```typescript
// Wei to Ether
const ether = ethers.formatEther(weiAmount);

// Ether to Wei
const wei = ethers.parseEther('1.0');

// Custom decimals
const tokens = ethers.formatUnits(amount, 6); // USDC
const amount = ethers.parseUnits('100', 6);
```

### Contract Interaction

```typescript
const contract = new ethers.Contract(
  contractAddress,
  abi,
  signerOrProvider
);

// Read
const result = await contract.methodName();

// Write
const tx = await contract.methodName(arg1, arg2);
await tx.wait();
```

### Transaction Details

```typescript
const tx = await provider.getTransaction(txHash);
console.log(tx);

const receipt = await provider.getTransactionReceipt(txHash);
console.log(receipt);
```

### Gas Estimation

```typescript
const gasEstimate = await contract.methodName.estimateGas(args);
console.log('Estimated gas:', gasEstimate.toString());
```

## Security Best Practices

1. **Never commit private keys**
   ```typescript
   // Use environment variables
   const wallet = new ethers.Wallet(process.env.PRIVATE_KEY);
   ```

2. **Validate addresses**
   ```typescript
   if (!ethers.isAddress(address)) {
     throw new Error('Invalid address');
   }
   ```

3. **Check transaction success**
   ```typescript
   const receipt = await tx.wait();
   if (receipt.status === 0) {
     throw new Error('Transaction failed');
   }
   ```

4. **Use try-catch**
   ```typescript
   try {
     await contract.methodName();
   } catch (error) {
     console.error('Transaction failed:', error);
   }
   ```

## Error Handling

```typescript
try {
  const tx = await wallet.sendTransaction({ ... });
  await tx.wait();
} catch (error) {
  if (error.code === 'INSUFFICIENT_FUNDS') {
    console.error('Insufficient balance');
  } else if (error.code === 'NONCE_EXPIRED') {
    console.error('Nonce issue');
  } else {
    console.error('Transaction failed:', error);
  }
}
```

## Testing

```typescript
// Use Hardhat or Ganache for local testing
const provider = new ethers.JsonRpcProvider('http://localhost:8545');
```

## Resources

- [Ethers.js Documentation](https://docs.ethers.org/)
- [Ethers.js GitHub](https://github.com/ethers-io/ethers.js)
- [Ethereum JSON-RPC](https://ethereum.org/en/developers/docs/apis/json-rpc/)

## Dependencies

- `ethers`: ^6.9.0
- `typescript`: ^5.3.0

## Stack

- Node.js 18+
- TypeScript 5
- Ethers.js 6
