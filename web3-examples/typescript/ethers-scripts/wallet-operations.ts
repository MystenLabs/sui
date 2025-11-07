import { ethers } from 'ethers';

/**
 * Ethers.js Wallet Operations
 * Demonstrates core Web3 operations using ethers.js
 */

// Provider setup (multiple options)
export function getProvider(rpcUrl?: string) {
  if (rpcUrl) {
    return new ethers.JsonRpcProvider(rpcUrl);
  }

  // Browser environment with MetaMask
  if (typeof window !== 'undefined' && window.ethereum) {
    return new ethers.BrowserProvider(window.ethereum);
  }

  // Default to Ethereum mainnet
  return ethers.getDefaultProvider('mainnet');
}

/**
 * Create a new wallet
 */
export function createWallet(): ethers.Wallet {
  const wallet = ethers.Wallet.createRandom();

  console.log('New Wallet Created:');
  console.log('Address:', wallet.address);
  console.log('Private Key:', wallet.privateKey);
  console.log('Mnemonic:', wallet.mnemonic?.phrase);

  return wallet;
}

/**
 * Import wallet from private key
 */
export function importWallet(privateKey: string, provider?: ethers.Provider): ethers.Wallet {
  const wallet = new ethers.Wallet(privateKey);

  if (provider) {
    return wallet.connect(provider);
  }

  return wallet;
}

/**
 * Get wallet balance
 */
export async function getBalance(
  address: string,
  provider: ethers.Provider
): Promise<string> {
  const balance = await provider.getBalance(address);
  return ethers.formatEther(balance);
}

/**
 * Send ETH transaction
 */
export async function sendEther(
  wallet: ethers.Wallet,
  toAddress: string,
  amount: string
): Promise<ethers.TransactionResponse> {
  const tx = await wallet.sendTransaction({
    to: toAddress,
    value: ethers.parseEther(amount),
  });

  console.log('Transaction sent:', tx.hash);
  console.log('Waiting for confirmation...');

  await tx.wait();
  console.log('Transaction confirmed!');

  return tx;
}

/**
 * Sign a message
 */
export async function signMessage(
  wallet: ethers.Wallet,
  message: string
): Promise<string> {
  const signature = await wallet.signMessage(message);
  console.log('Signature:', signature);
  return signature;
}

/**
 * Verify a signature
 */
export function verifySignature(
  message: string,
  signature: string
): string {
  const recoveredAddress = ethers.verifyMessage(message, signature);
  console.log('Recovered address:', recoveredAddress);
  return recoveredAddress;
}

/**
 * ERC20 Token Operations
 */
export class ERC20Token {
  private contract: ethers.Contract;

  private static ABI = [
    'function name() view returns (string)',
    'function symbol() view returns (string)',
    'function decimals() view returns (uint8)',
    'function totalSupply() view returns (uint256)',
    'function balanceOf(address) view returns (uint256)',
    'function transfer(address to, uint256 amount) returns (bool)',
    'function allowance(address owner, address spender) view returns (uint256)',
    'function approve(address spender, uint256 amount) returns (bool)',
    'function transferFrom(address from, address to, uint256 amount) returns (bool)',
  ];

  constructor(
    tokenAddress: string,
    signerOrProvider: ethers.Signer | ethers.Provider
  ) {
    this.contract = new ethers.Contract(
      tokenAddress,
      ERC20Token.ABI,
      signerOrProvider
    );
  }

  async getInfo() {
    const [name, symbol, decimals, totalSupply] = await Promise.all([
      this.contract.name(),
      this.contract.symbol(),
      this.contract.decimals(),
      this.contract.totalSupply(),
    ]);

    return {
      name,
      symbol,
      decimals: Number(decimals),
      totalSupply: ethers.formatUnits(totalSupply, decimals),
    };
  }

  async balanceOf(address: string): Promise<string> {
    const decimals = await this.contract.decimals();
    const balance = await this.contract.balanceOf(address);
    return ethers.formatUnits(balance, decimals);
  }

  async transfer(to: string, amount: string): Promise<ethers.TransactionResponse> {
    const decimals = await this.contract.decimals();
    const amountInWei = ethers.parseUnits(amount, decimals);

    const tx = await this.contract.transfer(to, amountInWei);
    await tx.wait();

    return tx;
  }

  async approve(spender: string, amount: string): Promise<ethers.TransactionResponse> {
    const decimals = await this.contract.decimals();
    const amountInWei = ethers.parseUnits(amount, decimals);

    const tx = await this.contract.approve(spender, amountInWei);
    await tx.wait();

    return tx;
  }
}

/**
 * Listen to events
 */
export async function listenToTransfers(
  tokenAddress: string,
  provider: ethers.Provider
) {
  const contract = new ethers.Contract(
    tokenAddress,
    ['event Transfer(address indexed from, address indexed to, uint256 value)'],
    provider
  );

  console.log('Listening for Transfer events...');

  contract.on('Transfer', (from, to, value, event) => {
    console.log(`Transfer detected:`);
    console.log(`  From: ${from}`);
    console.log(`  To: ${to}`);
    console.log(`  Value: ${ethers.formatEther(value)}`);
    console.log(`  Block: ${event.log.blockNumber}`);
  });
}

/**
 * Example usage
 */
export async function main() {
  // Create provider
  const provider = getProvider('https://eth.llamarpc.com');

  // Get balance
  const vitalikAddress = '0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045';
  const balance = await getBalance(vitalikAddress, provider);
  console.log(`Vitalik's balance: ${balance} ETH`);

  // Create new wallet
  const wallet = createWallet();

  // Sign message
  const message = 'Hello Web3!';
  const signature = await signMessage(wallet, message);

  // Verify signature
  const recovered = verifySignature(message, signature);
  console.log('Signature valid:', recovered === wallet.address);
}

// Run if executed directly
if (require.main === module) {
  main().catch(console.error);
}
