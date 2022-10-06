# Sui Wallet Adapter

> **⚠️ These packages are experimental and will change rapidly as they are being developed. Do not consider these APIs to be stable. If you have any feedback, [open an issue](https://github.com/MystenLabs/sui/issues/new/choose) or message us on [Discord](https://discord.gg/Sui).**

Sui Wallet Adapter is a set of libraries that makes it easy to connect your dApp to Sui wallets.

## Getting started

To get started in a React application, you can install the following packages:

```bash
npm install @mysten/wallet-adapter-all-wallets @mysten/wallet-adapter-react @mysten/wallet-adapter-react-ui
```

At the root of your application, you can then set up the wallet providers:

```tsx
import { WalletProvider } from "@mysten/wallet-adapter-react";
import { WalletStandardAdapterProvider } from "@mysten/wallet-adapter-all-wallets";

export function App() {
  const supportedWallets = [
    // Add support for all wallets that adhere to the Wallet Standard:
    new WalletStandardAdapterProvider(),
  ];

  return (
    <WalletProvider supportedWallets={supportedWallets}>
      {/* Your application... */}
    </WalletProvider>
  );
}
```

To add a **Connect Wallet** button to your page, use the `@mysten/wallet-adapter-react-ui` package:

```tsx
import { WalletWrapper } from "@mysten/wallet-adapter-react-ui";

function ConnectToWallet() {
  return <WalletWrapper />;
}
```

To get access to the currently connected wallet, use the `useWallet()` hook from `@mysten/wallet-adapter-react` to interact with the wallet, such as proposing transactions:

```tsx
import { useWallet } from "sui-wallet-adapter-react";

export function SendTransaction() {
  const { connected, getAccounts, signAndExecuteTransaction } = useWallet();

  const handleClick = async () => {
    await signAndExecuteTransaction({
      kind: "moveCall",
      data: {
        packageObjectId: "0x2",
        module: "devnet_nft",
        function: "mint",
        typeArguments: [],
        arguments: [
          "name",
          "capy",
          "https://cdn.britannica.com/94/194294-138-B2CF7780/overview-capybara.jpg?w=800&h=450&c=crop",
        ],
        gasBudget: 10000,
      },
    });
  };

  return (
    <Button onClick={handleClick} disabled={!connected}>
      Send Transaction
    </Button>
  );
}
```

### Usage without React

We do not currently have non-React UI libraries for connecting to wallets. The wallet adapters and logic in the React library can be used as reference for implementing a wallet connection UI in other UI libraries.

## Supported wallets

All available wallet adapters are currently exported via the `@mysten/wallet-adapter-all-wallets` package.
You can also install individual wallet adapters that you plan on using in your project.

### Wallet Standard

The `WalletStandardAdapterProvider` adapter (published under `@mysten/wallet-adapter-wallet-standard`) automatically supports wallets that adhere to the cross-chain [Wallet Standard](https://github.com/wallet-standard/wallet-standard/). This adapter detects the available wallets in users' browsers. You do not need to configure additional adapters.

The following wallets are known to work with the Wallet Standard:

- **[Sui Wallet](https://docs.sui.io/devnet/explore/wallet-browser)**

## Demo app

This repo has a simple demo app to test the behavior of the wallet adapters. You can run it using the following commands:

```bash
pnpm install
pnpm dev
```
