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
  preferredWallets?: string[];
}

export enum WalletKitCoreConnectionStatus {
  DISCONNECTED = "DISCONNECTED",
  CONNECTING = "CONNECTING",
  CONNECTED = "CONNECTED",
  // TODO: Figure out if this is really a separate status, or is just a piece of state alongside the `disconnected` state:
  ERROR = "ERROR",
}

export interface InternalWalletKitCoreState {
  wallets: WalletAdapter[];
  currentWallet: WalletAdapter | null;
  accounts: SuiAddress[];
  currentAccount: SuiAddress | null;
  status: WalletKitCoreConnectionStatus;
}

export interface WalletKitCoreState extends InternalWalletKitCoreState {
  isConnecting: boolean;
  isConnected: boolean;
  isError: boolean;
}

export interface WalletKitCore {
  getState(): WalletKitCoreState;
  subscribe(handler: SubscribeHandler): Unsubscribe;
  connect(walletName: string): Promise<void>;
  disconnect(): Promise<void>;
  signAndExecuteTransaction(
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse>;
}

export type SubscribeHandler = (state: WalletKitCoreState) => void;
export type Unsubscribe = () => void;

const SUI_WALLET_NAME = "Sui Wallet";

function sortWallets(wallets: WalletAdapter[], preferredWallets: string[]) {
  return [
    // Preferred wallets, in order:
    ...(preferredWallets
      .map((name) => wallets.find((wallet) => wallet.name === name))
      .filter(Boolean) as WalletAdapter[]),

    // Wallets in default order:
    ...wallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
  ];
}

// TODO: Support autoconnect.
// TODO: Support lazy loaded adapters, where we'll resolve the adapters only once we attempt to use them.
// That should allow us to have effective code-splitting practices. We should also allow lazy loading of _many_
// wallet adapters in one bag so that we can split _all_ of the adapters from the core.
export function createWalletKitCore({
  adapters,
  preferredWallets = [SUI_WALLET_NAME],
}: WalletKitCoreOptions): WalletKitCore {
  const subscriptions: Set<(state: WalletKitCoreState) => void> = new Set();
  let walletEventUnsubscribe: (() => void) | null = null;

  let internalState: InternalWalletKitCoreState = {
    accounts: [],
    currentAccount: null,
    wallets: sortWallets(resolveAdapters(adapters), preferredWallets),
    currentWallet: null,
    status: WalletKitCoreConnectionStatus.DISCONNECTED,
  };

  const computeState = () => ({
    ...internalState,
    isConnecting:
      internalState.status === WalletKitCoreConnectionStatus.CONNECTING,
    isConnected:
      internalState.status === WalletKitCoreConnectionStatus.CONNECTED,
    isError: internalState.status === WalletKitCoreConnectionStatus.ERROR,
  });

  let state = computeState();

  function setState(nextInternalState: Partial<InternalWalletKitCoreState>) {
    internalState = {
      ...internalState,
      ...nextInternalState,
    };
    state = computeState();
    subscriptions.forEach((handler) => {
      try {
        handler(state);
      } catch {}
    });
  }

  function disconnected() {
    if (walletEventUnsubscribe) {
      walletEventUnsubscribe();
      walletEventUnsubscribe = null;
    }
    setState({
      status: WalletKitCoreConnectionStatus.DISCONNECTED,
      accounts: [],
      currentAccount: null,
      currentWallet: null,
    });
  }

  // TODO: Defer this somehow, probably alongside the work above for lazy wallet adapters:
  const providers = adapters.filter(isWalletProvider);
  if (providers.length) {
    providers.map((provider) =>
      provider.on("changed", () => {
        setState({
          wallets: sortWallets(resolveAdapters(adapters), preferredWallets),
        });
      })
    );
  }

  return {
    getState() {
      return state;
    },

    subscribe(handler) {
      subscriptions.add(handler);

      // Immediately invoke the handler with the current state to make it compatible with Svelte stores:
      try {
        handler(state);
      } catch {}

      return () => {
        subscriptions.delete(handler);
      };
    },

    async connect(walletName) {
      const currentWallet =
        internalState.wallets.find((wallet) => wallet.name === walletName) ??
        null;
      // TODO: Should the current wallet actually be set before we successfully connect to it?
      setState({ currentWallet });

      if (currentWallet && !currentWallet.connecting) {
        if (walletEventUnsubscribe) {
          walletEventUnsubscribe();
        }
        walletEventUnsubscribe = currentWallet.on("change", ({ connected }) => {
          // when undefined connected hasn't changed
          if (connected === false) {
            disconnected();
          }
        });
        try {
          setState({ status: WalletKitCoreConnectionStatus.CONNECTING });
          await currentWallet.connect();
          setState({ status: WalletKitCoreConnectionStatus.CONNECTED });
          // TODO: Rather than using this method, we should just standardize the wallet properties on the adapter itself:
          const accounts = await currentWallet.getAccounts();
          // TODO: Implement account selection:

          setState({ accounts, currentAccount: accounts[0] ?? null });
        } catch (e) {
          console.log("Wallet connection error", e);

          setState({ status: WalletKitCoreConnectionStatus.ERROR });
        }
      } else {
        setState({ status: WalletKitCoreConnectionStatus.DISCONNECTED });
      }
    },

    async disconnect() {
      if (!internalState.currentWallet) {
        console.warn("Attempted to `disconnect` but no wallet was connected.");
        return;
      }
      await internalState.currentWallet.disconnect();
      disconnected();
    },

    signAndExecuteTransaction(transaction) {
      if (!internalState.currentWallet) {
        throw new Error(
          "No wallet is currently connected, cannot call `signAndExecuteTransaction`."
        );
      }

      return internalState.currentWallet.signAndExecuteTransaction(transaction);
    },
  };
}
