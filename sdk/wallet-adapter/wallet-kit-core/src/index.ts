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
export class WalletKitCore {
  #wallets: WalletAdapter[];
  #currentWallet: WalletAdapter | null;
  #status: WalletKitCoreConnectionStatus;

  #accounts: SuiAddress[];
  #currentAccount: SuiAddress | null;

  #state: WalletKitCoreState;
  #subscriptions: Set<(state: WalletKitCoreState) => void>;

  constructor({ adapters }: WalletKitCoreOptions) {
    this.#wallets = resolveAdapters(adapters);
    this.#currentWallet = null;
    this.#accounts = [];
    this.#currentAccount = null;
    this.#status = WalletKitCoreConnectionStatus.DISCONNECTED;
    this.#subscriptions = new Set();
    this.#state = this.#computeState();

    // TODO: Defer this somehow, probably alongside the work above for lazy wallet adapters:
    const providers = adapters.filter(isWalletProvider);
    if (providers.length) {
      providers.map((provider) =>
        provider.on("changed", () => {
          this.#setWallets(resolveAdapters(adapters));
        })
      );
    }
  }

  subscribe = (handler: SubscribeHandler): Unsubscribe => {
    // Immediately invoke the handler with the current state to make it compatible with Svelte stores:
    handler(this.getState());
    this.#subscriptions.add(handler);
    return () => {
      this.#subscriptions.delete(handler);
    };
  };

  getState = (): WalletKitCoreState => {
    return this.#state;
  };

  #computeState(): WalletKitCoreState {
    return {
      accounts: this.#accounts,
      currentAccount: this.#currentAccount,
      wallets: this.#wallets,
      currentWallet: this.#currentWallet,
      status: this.#status,
      isConnecting: this.#status === WalletKitCoreConnectionStatus.CONNECTING,
      isConnected: this.#status === WalletKitCoreConnectionStatus.CONNECTED,
      isError: this.#status === WalletKitCoreConnectionStatus.ERROR,
    };
  }

  // TODO: Try-catch to make more robust
  #notify() {
    this.#state = this.#computeState();
    this.#subscriptions.forEach((handler) => handler(this.#state));
  }

  #setStatus(status: WalletKitCoreConnectionStatus) {
    this.#status = status;
    this.#notify();
  }

  #setCurrentWallet(currentWallet: WalletAdapter | null) {
    this.#currentWallet = currentWallet;
    this.#notify();
  }

  #setWallets(wallets: WalletAdapter[]) {
    this.#wallets = wallets;
    this.#notify();
  }

  #setAccounts(accounts: SuiAddress[], currentAccount: SuiAddress | null) {
    this.#accounts = accounts;
    this.#currentAccount = currentAccount;
    this.#notify();
  }

  // TODO: Handle this being called multiple times:
  // TODO: Return an abort controller so that they can be cancelled.
  // TODO: Handle already connecting state better.
  connect = async (walletName: string) => {
    const currentWallet =
      this.#wallets.find((wallet) => wallet.name === walletName) ?? null;

    // TODO: Should the current wallet actually be set before we successfully connect to it?
    this.#setCurrentWallet(currentWallet);

    if (currentWallet && !currentWallet.connecting) {
      try {
        this.#setStatus(WalletKitCoreConnectionStatus.CONNECTING);
        await currentWallet.connect();
        this.#setStatus(WalletKitCoreConnectionStatus.CONNECTED);
        // TODO: Rather than using this method, we should just standardize the wallet properties on the adapter itself:
        const accounts = await currentWallet.getAccounts();
        // TODO: Implement account selection:
        this.#setAccounts(accounts, accounts[0] ?? null);
      } catch (e) {
        console.log("Wallet connection error", e);
        this.#setStatus(WalletKitCoreConnectionStatus.ERROR);
      }
    } else {
      this.#setStatus(WalletKitCoreConnectionStatus.DISCONNECTED);
    }
  };

  disconnect = () => {
    if (!this.#currentWallet) {
      console.warn("Attempted to `disconnect` but no wallet was connected.");
      return;
    }

    this.#currentWallet.disconnect();
    this.#setStatus(WalletKitCoreConnectionStatus.DISCONNECTED);
    this.#setAccounts([], null);
    this.#setCurrentWallet(null);
  };

  signAndExecuteTransaction = (
    transaction: SignableTransaction
  ): Promise<SuiTransactionResponse> => {
    if (!this.#currentWallet) {
      throw new Error(
        "No wallet is currently connected, cannot call `signAndExecuteTransaction`."
      );
    }

    return this.#currentWallet.signAndExecuteTransaction(transaction);
  };
}
