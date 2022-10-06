# `@mysten/wallet-standard`

A suite of standard utilities for implementing wallets and libraries based on the [Wallet Standard](https://github.com/wallet-standard/wallet-standard/).

## Implementing the Wallet Standard in an extension wallet

### Creating a wallet interface

You need to create a class that represents your wallet. You can use the `Wallet` interface from `@mysten/wallet-standard` to help ensure your class adheres to the standard.

```typescript
import { Wallet, SUI_DEVNET_CHAIN } from "@mysten/wallet-standard";

class YourWallet implements Wallet {
  get version() {
    // Return the version of the Wallet Standard this implements (in this case, 1.0.0).
    return "1.0.0";
  }
  get name() {
    return "Wallet Name";
  }
  get icon() {
    return "some-icon-data-url";
  }

  // Return the Sui chains that your wallet supports.
  get chains() {
    return [SUI_DEVNET_CHAIN];
  }
}
```

### Implementing features

Features are standard methods that can be used by consumers to interact with the wallet. For Sui wallets, we expect the following features to be implemented:

- `standard:connect` - Used to initiate a connection to the wallet.
- `sui:signAndExecuteTransaction` - Used to prompt the user to sign a transaction, then submit it for execution to the blockchain.

You can implement these features in your wallet class under the `features` property:

```typescript
import {
  ConnectFeature,
  SuiSignAndExecuteTransactionFeature,
} from "@mysten/wallet-standard";

class YourWallet implements Wallet {
  get features(): ConnectFeature & SuiSignAndExecuteTransactionFeature {
    return {
      "standard:connect": {
        version: "1.0.0",
        connect: this.#connect,
      },
      "sui:signAndExecuteTransaction": {
        version: "1.0.0",
        signAndExecuteTransaction: this.#signAndExecuteTransaction,
      },
    };
  },

	#connect() {
		// Your wallet's connect implementation
	}

	#signAndExecuteTransaction() {
		// Your wallet's signAndExecuteTransaction implementation
	}
}
```

### Exposing accounts

The last requirement of the wallet interface is to expose an `acccounts` interface. This should expose all of the accounts that a connected dapp has access to. It can be empty prior to initiating a connection through the `standard:connect` feature.

The accounts can use the `ReadonlyWalletAccount` class to easily construct an account matching the required interface.

```typescript
import { ReadonlyWalletAccount } from "@mysten/wallet-standard";

class YourWallet implements Wallet {
  get accounts() {
    // Assuming we already have some internal representation of accounts:
    return someWalletAccounts.map(
      (walletAccount) =>
        // Return
        new ReadonlyWalletAccount({
          address: walletAccount.suiAddress,
          publicKey: walletAccount.pubkey,
          // The Sui chains that your wallet supports.
          chains: [SUI_DEVNET_CHAIN],
          // The features that this account supports. This can be a subset of the wallet's supported features.
          // These features must exist on the wallet as well.
          features: ["sui:signAndExecuteTransaction", "standard:signMessage"],
        })
    );
  }
}
```

### Registering in the window

Once you have a compatible interface for your wallet, you can register it in the window under the `window.navigator.wallets` interface. This is an array-like interface where all wallets self-register by pushing their wallet into.

```typescript
// This makes TypeScript aware of the `window.navigator.wallets` interface.
declare const window: import("@mysten/wallet-standard").WalletsWindow;

(window.navigator.wallets || []).push(({ register }) => {
  register(new YourWallet());
});
```

> Note that while this interface is array-like, it is not always an array, and the only method that should be called on it is `push`.
