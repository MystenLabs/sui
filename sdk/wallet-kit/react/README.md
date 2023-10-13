# Sui Wallet Kit

> **⚠️ These packages are experimental and will change rapidly as they are being developed. Do not
> consider these APIs to be stable. If you have any feedback,
> [open an issue](https://github.com/MystenLabs/sui/issues/new/choose) or message us on
> [Discord](https://discord.gg/Sui).**

Sui Wallet Kit is a library that makes it easy to connect your dApp to Sui wallets.

## Getting started

To get started in a React application, you can install the following packages:

```bash
npm install @mysten/wallet-kit
```

At the root of your application, you can then set up the wallet kit provider:

```tsx
import { WalletKitProvider } from '@mysten/wallet-kit';

export function App() {
	return <WalletKitProvider>{/* Your application... */}</WalletKitProvider>;
}
```

You can then add a **Connect Wallet** button to your page:

```tsx
import { formatAddress } from '@mysten/sui.js/utils';
import { ConnectButton, useWalletKit } from '@mysten/wallet-kit';

function ConnectToWallet() {
	const { currentAccount } = useWalletKit();
	return (
		<ConnectButton
			connectText={'Connect Wallet'}
			connectedText={`Connected: ${formatAddress(currentAccount.address)}`}
		/>
	);
}
```

To get access to the currently connected wallet, use the `useWalletKit()` hook to interact with the
wallet, such as proposing transactions:

```tsx
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { useWalletKit } from '@mysten/wallet-kit';

export function SendTransaction() {
	const { signAndExecuteTransactionBlock } = useWalletKit();

	const handleClick = async () => {
		const tx = new TransactionBlock();
		tx.moveCall({
			target: '0x2::devnet_nft::mint',
			arguments: [
				tx.pure.string('some name'),
				tx.pure.string('some description'),
				tx.pure.string(
					'https://cdn.britannica.com/94/194294-138-B2CF7780/overview-capybara.jpg?w=800&h=450&c=crop',
				),
			],
		});
		await signAndExecuteTransactionBlock({ transactionBlock: tx });
	};

	return (
		<Button onClick={handleClick} disabled={!connected}>
			Send Transaction
		</Button>
	);
}
```

### Usage without React

We do not currently have non-React UI libraries for connecting to wallets. The logic in the React
library (`@mysten/wallet-kit`) can be used as reference for implementing a wallet connection UI in
other UI libraries.

## Supported wallets

Wallet Kit comes pre-configured to support all wallets adhering to the
[Wallet Standard](https://github.com/wallet-standard/wallet-standard/).
