// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  WalletAdapterList,
  resolveAdapters,
  WalletAdapter,
  isWalletProvider,
} from "@mysten/wallet-adapter-base";
import { localStorageAdapter, StorageAdapter } from "./storage";
import {
  SuiSignAndExecuteTransactionBlockInput,
  SuiSignMessageInput,
  SuiSignTransactionBlockInput,
  WalletAccount,
} from "@mysten/wallet-standard";

export * from "./storage";

export interface WalletKitCoreOptions {
  adapters: WalletAdapterList;
  preferredWallets?: string[];
  storageAdapter?: StorageAdapter;
  storageKey?: string;
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
  accounts: readonly WalletAccount[];
  currentAccount: WalletAccount | null;
  status: WalletKitCoreConnectionStatus;
}

export interface WalletKitCoreState extends InternalWalletKitCoreState {
  isConnecting: boolean;
  isConnected: boolean;
  isError: boolean;
}

type OptionalProperties<T extends Record<any, any>, U extends keyof T> = Omit<
  T,
  U
> &
  Partial<Pick<T, U>>;

export interface WalletKitCore {
  autoconnect(): Promise<void>;
  getState(): WalletKitCoreState;
  subscribe(handler: SubscribeHandler): Unsubscribe;
  connect(walletName: string): Promise<void>;
  selectAccount(account: WalletAccount): void;
  disconnect(): Promise<void>;
  signMessage(
    messageInput: OptionalProperties<SuiSignMessageInput, "account">
  ): ReturnType<WalletAdapter["signMessage"]>;
  signTransactionBlock: (
    transactionInput: OptionalProperties<
      SuiSignTransactionBlockInput,
      "chain" | "account"
    >
  ) => ReturnType<WalletAdapter["signTransactionBlock"]>;
  signAndExecuteTransactionBlock: (
    transactionInput: OptionalProperties<
      SuiSignAndExecuteTransactionBlockInput,
      "chain" | "account"
    >
  ) => ReturnType<WalletAdapter["signAndExecuteTransactionBlock"]>;
}

export type SubscribeHandler = (state: WalletKitCoreState) => void;
export type Unsubscribe = () => void;

const SUI_WALLET_NAME = "Sui Wallet";

const RECENT_WALLET_STORAGE = "wallet-kit:last-wallet";

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

// TODO: Support lazy loaded adapters, where we'll resolve the adapters only once we attempt to use them.
// That should allow us to have effective code-splitting practices. We should also allow lazy loading of _many_
// wallet adapters in one bag so that we can split _all_ of the adapters from the core.
export function createWalletKitCore({
  adapters,
  preferredWallets = [SUI_WALLET_NAME],
  storageAdapter = localStorageAdapter,
  storageKey = RECENT_WALLET_STORAGE,
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

  const walletKit: WalletKitCore = {
    async autoconnect() {
      if (state.currentWallet) return;

      try {
        const lastWalletName = await storageAdapter.get(storageKey);
        if (lastWalletName) {
          walletKit.connect(lastWalletName);
        }
      } catch {}
    },

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

    selectAccount(account) {
      if (
        account === internalState.currentAccount ||
        !internalState.accounts.includes(account)
      ) {
        return;
      }

      setState({
        currentAccount: account,
      });
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
        walletEventUnsubscribe = currentWallet.on(
          "change",
          ({ connected, accounts }) => {
            // when undefined connected hasn't changed
            if (connected === false) {
              disconnected();
            } else if (accounts) {
              setState({
                accounts,
                currentAccount:
                  internalState.currentAccount &&
                  !accounts.find(
                    ({ address }) =>
                      address === internalState.currentAccount?.address
                  )
                    ? accounts[0]
                    : internalState.currentAccount,
              });
            }
          }
        );
        try {
          setState({ status: WalletKitCoreConnectionStatus.CONNECTING });
          await currentWallet.connect();
          setState({ status: WalletKitCoreConnectionStatus.CONNECTED });
          try {
            await storageAdapter.set(storageKey, currentWallet.name);
          } catch {}
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
      try {
        await storageAdapter.del(storageKey);
      } catch {}
      await internalState.currentWallet.disconnect();
      disconnected();
    },

    signMessage(messageInput) {
      if (!internalState.currentWallet || !internalState.currentAccount) {
        throw new Error(
          "No wallet is currently connected, cannot call `signMessage`."
        );
      }

      return internalState.currentWallet.signMessage({
        ...messageInput,
        account: messageInput.account ?? internalState.currentAccount,
      });
    },

    async signTransactionBlock(transactionInput) {
      if (!internalState.currentWallet || !internalState.currentAccount) {
        throw new Error(
          "No wallet is currently connected, cannot call `signTransaction`."
        );
      }
      const {
        account = internalState.currentAccount,
        chain = internalState.currentAccount.chains[0],
      } = transactionInput;
      if (!chain) {
        throw new Error("Missing chain");
      }
      return internalState.currentWallet.signTransactionBlock({
        ...transactionInput,
        account,
        chain,
      });
    },

    async signAndExecuteTransactionBlock(transactionInput) {
      if (!internalState.currentWallet || !internalState.currentAccount) {
        throw new Error(
          "No wallet is currently connected, cannot call `signAndExecuteTransactionBlock`."
        );
      }
      const {
        account = internalState.currentAccount,
        chain = internalState.currentAccount.chains[0],
      } = transactionInput;
      if (!chain) {
        throw new Error("Missing chain");
      }
      return internalState.currentWallet.signAndExecuteTransactionBlock({
        ...transactionInput,
        account,
        chain,
      });
    },
  };

  return walletKit;
}
