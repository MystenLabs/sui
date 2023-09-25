// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	getWallets,
	isWalletWithSuiFeatures,
	StandardConnectInput,
	SuiSignAndExecuteTransactionBlockInput,
	SuiSignAndExecuteTransactionBlockOutput,
	SuiSignMessageInput,
	SuiSignMessageOutput,
	SuiSignPersonalMessageInput,
	SuiSignPersonalMessageOutput,
	SuiSignTransactionBlockInput,
	SuiSignTransactionBlockOutput,
	Wallet,
	WalletAccount,
	WalletWithSuiFeatures,
} from '@mysten/wallet-standard';

import { localStorageAdapter, StorageAdapter } from './storage';

export * from './storage';

export const DEFAULT_FEATURES: (keyof WalletWithSuiFeatures['features'])[] = [
	'sui:signAndExecuteTransactionBlock',
];

export interface WalletKitCoreOptions {
	preferredWallets?: string[];
	storageAdapter?: StorageAdapter;
	storageKey?: string;
	features?: string[];
}

export enum WalletKitCoreConnectionStatus {
	DISCONNECTED = 'DISCONNECTED',
	CONNECTING = 'CONNECTING',
	CONNECTED = 'CONNECTED',
	// TODO: Figure out if this is really a separate status, or is just a piece of state alongside the `disconnected` state:
	ERROR = 'ERROR',
}

export interface InternalWalletKitCoreState {
	wallets: WalletWithSuiFeatures[];
	currentWallet: WalletWithSuiFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	status: WalletKitCoreConnectionStatus;
}

export interface WalletKitCoreState extends InternalWalletKitCoreState {
	isConnecting: boolean;
	isConnected: boolean;
	isError: boolean;
}

type OptionalProperties<T extends Record<any, any>, U extends keyof T> = Omit<T, U> &
	Partial<Pick<T, U>>;

export interface WalletKitCore {
	autoconnect(): Promise<void>;
	getState(): WalletKitCoreState;
	subscribe(handler: SubscribeHandler): Unsubscribe;
	connect(walletName: string, connectInput?: StandardConnectInput): Promise<void>;
	selectAccount(account: WalletAccount): void;
	disconnect(): Promise<void>;
	/** @deprecated Use `signPersonalMessage` instead. */
	signMessage(
		messageInput: OptionalProperties<SuiSignMessageInput, 'account'>,
	): Promise<SuiSignMessageOutput>;
	signPersonalMessage(
		messageInput: OptionalProperties<SuiSignPersonalMessageInput, 'account'>,
	): Promise<SuiSignPersonalMessageOutput>;
	signTransactionBlock: (
		transactionInput: OptionalProperties<SuiSignTransactionBlockInput, 'chain' | 'account'>,
	) => Promise<SuiSignTransactionBlockOutput>;
	signAndExecuteTransactionBlock: (
		transactionInput: OptionalProperties<
			SuiSignAndExecuteTransactionBlockInput,
			'chain' | 'account'
		>,
	) => Promise<SuiSignAndExecuteTransactionBlockOutput>;
}

export type SubscribeHandler = (state: WalletKitCoreState) => void;
export type Unsubscribe = () => void;

const SUI_WALLET_NAME = 'Sui Wallet';

const RECENT_WALLET_STORAGE = 'wallet-kit:last-wallet';

function waitToBeVisible() {
	if (!document || document.visibilityState === 'visible') {
		return Promise.resolve();
	}
	let promiseResolve: (() => void) | null = null;
	const promise = new Promise<void>((r) => (promiseResolve = r));
	const callback = () => {
		if (promiseResolve && document.visibilityState === 'visible') {
			promiseResolve();
			document.removeEventListener('visibilitychange', callback);
		}
	};
	document.addEventListener('visibilitychange', callback);
	return promise;
}

function sortWallets(
	wallets: readonly Wallet[],
	preferredWallets: string[],
	features?: string[],
): WalletWithSuiFeatures[] {
	const suiWallets = wallets.filter((wallet) =>
		isWalletWithSuiFeatures(wallet, features),
	) as WalletWithSuiFeatures[];

	return [
		// Preferred wallets, in order:
		...(preferredWallets
			.map((name) => suiWallets.find((wallet) => wallet.name === name))
			.filter(Boolean) as WalletWithSuiFeatures[]),

		// Wallets in default order:
		...suiWallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
	];
}

export function createWalletKitCore({
	preferredWallets = [SUI_WALLET_NAME],
	storageAdapter = localStorageAdapter,
	storageKey = RECENT_WALLET_STORAGE,
	features = DEFAULT_FEATURES,
}: WalletKitCoreOptions): WalletKitCore {
	const registeredWallets = getWallets();
	let wallets = registeredWallets.get();

	const subscriptions: Set<(state: WalletKitCoreState) => void> = new Set();
	let walletEventUnsubscribe: (() => void) | null = null;

	let internalState: InternalWalletKitCoreState = {
		accounts: [],
		currentAccount: null,
		wallets: sortWallets(wallets, preferredWallets, features),
		currentWallet: null,
		status: WalletKitCoreConnectionStatus.DISCONNECTED,
	};

	const computeState = () => ({
		...internalState,
		isConnecting: internalState.status === WalletKitCoreConnectionStatus.CONNECTING,
		isConnected: internalState.status === WalletKitCoreConnectionStatus.CONNECTED,
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
			} catch {
				/* ignore error */
			}
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

	const handleWalletsChanged = () => {
		setState({
			wallets: sortWallets(registeredWallets.get(), preferredWallets, features),
		});
	};

	registeredWallets.on('register', handleWalletsChanged);
	registeredWallets.on('unregister', handleWalletsChanged);

	const walletKit: WalletKitCore = {
		async autoconnect() {
			if (state.currentWallet) return;
			await waitToBeVisible();
			try {
				const lastWalletName = await storageAdapter.get(storageKey);
				if (lastWalletName) {
					walletKit.connect(lastWalletName, { silent: true });
				}
			} catch {
				/* ignore error */
			}
		},

		getState() {
			return state;
		},

		subscribe(handler) {
			subscriptions.add(handler);

			// Immediately invoke the handler with the current state to make it compatible with Svelte stores:
			try {
				handler(state);
			} catch {
				/* ignore error */
			}

			return () => {
				subscriptions.delete(handler);
			};
		},

		selectAccount(account) {
			if (account === internalState.currentAccount || !internalState.accounts.includes(account)) {
				return;
			}

			setState({
				currentAccount: account,
			});
		},

		async connect(walletName, connectInput) {
			const currentWallet =
				internalState.wallets.find((wallet) => wallet.name === walletName) ?? null;

			// TODO: Should the current wallet actually be set before we successfully connect to it?
			setState({ currentWallet });

			if (currentWallet) {
				if (walletEventUnsubscribe) {
					walletEventUnsubscribe();
				}
				walletEventUnsubscribe = currentWallet.features['standard:events'].on(
					'change',
					({ accounts, features, chains }) => {
						// TODO: Handle features or chains changing.
						if (accounts) {
							setState({
								accounts,
								currentAccount:
									internalState.currentAccount &&
									!accounts.find(({ address }) => address === internalState.currentAccount?.address)
										? accounts[0]
										: internalState.currentAccount,
							});
						}
					},
				);

				try {
					setState({ status: WalletKitCoreConnectionStatus.CONNECTING });
					await currentWallet.features['standard:connect'].connect(connectInput);
					setState({ status: WalletKitCoreConnectionStatus.CONNECTED });
					try {
						await storageAdapter.set(storageKey, currentWallet.name);
					} catch {
						/* ignore error */
					}

					setState({
						accounts: currentWallet.accounts,
						currentAccount: currentWallet.accounts[0] ?? null,
					});
				} catch (e) {
					console.log('Wallet connection error', e);

					setState({ status: WalletKitCoreConnectionStatus.ERROR });
				}
			} else {
				setState({ status: WalletKitCoreConnectionStatus.DISCONNECTED });
			}
		},

		async disconnect() {
			if (!internalState.currentWallet) {
				console.warn('Attempted to `disconnect` but no wallet was connected.');
				return;
			}
			try {
				await storageAdapter.del(storageKey);
			} catch {
				/* ignore error */
			}
			await internalState.currentWallet.features['standard:disconnect']?.disconnect();
			disconnected();
		},

		/** @deprecated Use `signPersonalMessage` instead. */
		signMessage(messageInput) {
			if (!internalState.currentWallet || !internalState.currentAccount) {
				throw new Error('No wallet is currently connected, cannot call `signMessage`.');
			}

			if (!internalState.currentWallet.features['sui:signMessage']) {
				throw new Error('Wallet does not support deprecated `signMessage` method.');
			}

			return internalState.currentWallet.features['sui:signMessage'].signMessage({
				...messageInput,
				account: messageInput.account ?? internalState.currentAccount,
			});
		},

		signPersonalMessage(messageInput) {
			if (!internalState.currentWallet || !internalState.currentAccount) {
				throw new Error('No wallet is currently connected, cannot call `signPersonalMessage`.');
			}

			if (!internalState.currentWallet.features['sui:signPersonalMessage']) {
				throw new Error('Wallet does not support the new `signPersonalMessage` method.');
			}

			return internalState.currentWallet.features['sui:signPersonalMessage'].signPersonalMessage({
				...messageInput,
				account: messageInput.account ?? internalState.currentAccount,
			});
		},

		async signTransactionBlock(transactionInput) {
			if (!internalState.currentWallet || !internalState.currentAccount) {
				throw new Error('No wallet is currently connected, cannot call `signTransaction`.');
			}
			const {
				account = internalState.currentAccount,
				chain = internalState.currentAccount.chains[0],
			} = transactionInput;
			if (!chain) {
				throw new Error('Missing chain');
			}
			return internalState.currentWallet.features['sui:signTransactionBlock'].signTransactionBlock({
				...transactionInput,
				account,
				chain,
			});
		},

		async signAndExecuteTransactionBlock(transactionInput) {
			if (!internalState.currentWallet || !internalState.currentAccount) {
				throw new Error(
					'No wallet is currently connected, cannot call `signAndExecuteTransactionBlock`.',
				);
			}
			const {
				account = internalState.currentAccount,
				chain = internalState.currentAccount.chains[0],
			} = transactionInput;

			if (!chain) {
				throw new Error('Missing chain');
			}

			return internalState.currentWallet.features[
				'sui:signAndExecuteTransactionBlock'
			].signAndExecuteTransactionBlock({
				...transactionInput,
				account,
				chain,
			});
		},
	};

	return walletKit;
}
