// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  SignableTransaction,
  SuiAddress,
  SuiTransactionResponse,
} from "@mysten/sui.js";
import {
  WalletAdapterList,
  resolveAdapters,
  WalletAdapter,
  isWalletProvider,
} from "@mysten/wallet-adapter-base";

export interface WalletKitCoreOptions {
  adapters: WalletAdapterList;
}

export enum WalletKitCoreConnectionStatus {
  DISCONNECTED = "DISCONNECTED",
  CONNECTING = "CONNECTING",
  CONNECTED = "CONNECTED",
  // TODO: Figure out if this is really a separate status, or is just a piece of state alongside the `disconnected` state:
  ERROR = "ERROR",
}

export interface WalletKitCoreState {
  wallets: WalletAdapter[];
  currentWallet: WalletAdapter | null;
  accounts: SuiAddress[];
  currentAccount: SuiAddress | null;
  status: WalletKitCoreConnectionStatus;
  isConnecting: boolean;
  isConnected: boolean;
  isError: boolean;
}

export type SubscribeHandler = (state: WalletKitCoreState) => void;
export type Unsubscribe = () => void;

// TODO: Support autoconnect.
// TODO: Refactor away from classes so that binding is less of an issue:
// TODO: Support lazy loaded adapters, where we'll resolve the adapters only once we attempt to use them.
// That should allow us to have effective code-splitting practices. We should also allow lazy loading of _many_
// wallet adapters in one bag so that we can split _all_ of the adapters from the core.
export function createWalletKitCore({ adapters }: WalletKitCoreOptions) {
  let status = WalletKitCoreConnectionStatus.DISCONNECTED;

  let wallets: WalletAdapter[] = resolveAdapters(adapters);
  let accounts: SuiAddress[] = [];

  let currentWallet: WalletAdapter | null = null;
  let currentAccount: SuiAddress | null = null;

  const subscriptions: Set<(state: WalletKitCoreState) => void> = new Set();

  const computeState = () => ({
    accounts,
    currentAccount,
    wallets,
    currentWallet,
    status,
    isConnecting: status === WalletKitCoreConnectionStatus.CONNECTING,
    isConnected: status === WalletKitCoreConnectionStatus.CONNECTED,
    isError: status === WalletKitCoreConnectionStatus.ERROR,
  });

  let state: WalletKitCoreState = computeState();
  const setState = () => {
    state = computeState();
  };

  // TODO: Try-catch to make more robust
  function update() {
    state = computeState();
    subscriptions.forEach((handler) => handler(state));
  }

  // TODO: Defer this somehow, probably alongside the work above for lazy wallet adapters:
  const providers = adapters.filter(isWalletProvider);
  if (providers.length) {
    providers.map((provider) =>
      provider.on("changed", () => {
        wallets = resolveAdapters(adapters);
        update();
      })
    );
  }

  function setStatus(nextStatus: WalletKitCoreConnectionStatus) {
    status = nextStatus;
    update();
  }

  function setAccounts(
    nextAccounts: SuiAddress[],
    nextCurrentAccount: SuiAddress | null
  ) {
    accounts = nextAccounts;
    currentAccount = nextCurrentAccount;
    update();
  }

  function setCurrentWallet(nextCurrentWallet: WalletAdapter | null) {
    currentWallet = nextCurrentWallet;
    update();
  }

  return {
    getState() {
      return state;
    },

    subscribe(handler: SubscribeHandler): Unsubscribe {
      // Immediately invoke the handler with the current state to make it compatible with Svelte stores:
      handler(this.getState());
      subscriptions.add(handler);
      return () => {
        subscriptions.delete(handler);
      };
    },

    connect: async (walletName: string) => {
      const nextCurrentWallet =
        wallets.find((wallet) => wallet.name === walletName) ?? null;

      // TODO: Should the current wallet actually be set before we successfully connect to it?
      currentWallet = nextCurrentWallet;
      update();

      if (currentWallet && !currentWallet.connecting) {
        try {
          setStatus(WalletKitCoreConnectionStatus.CONNECTING);
          await currentWallet.connect();
          setStatus(WalletKitCoreConnectionStatus.CONNECTED);
          // TODO: Rather than using this method, we should just standardize the wallet properties on the adapter itself:
          const accounts = await currentWallet.getAccounts();
          // TODO: Implement account selection:

          setAccounts(accounts, accounts[0] ?? null);
        } catch (e) {
          console.log("Wallet connection error", e);
          setStatus(WalletKitCoreConnectionStatus.ERROR);
        }
      } else {
        setStatus(WalletKitCoreConnectionStatus.DISCONNECTED);
      }
    },

    disconnect: () => {
      if (!currentWallet) {
        console.warn("Attempted to `disconnect` but no wallet was connected.");
        return;
      }

      currentWallet.disconnect();
      setStatus(WalletKitCoreConnectionStatus.DISCONNECTED);
      setAccounts([], null);
      setCurrentWallet(null);
    },

    signAndExecuteTransaction: (
      transaction: SignableTransaction
    ): Promise<SuiTransactionResponse> => {
      if (!currentWallet) {
        throw new Error(
          "No wallet is currently connected, cannot call `signAndExecuteTransaction`."
        );
      }

      return currentWallet.signAndExecuteTransaction(transaction);
    },
  };
}
