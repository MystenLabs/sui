// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithSuiFeatures, WalletAccount, Wallet } from '@mysten/wallet-standard';
import { assertUnreachable } from 'dapp-kit/src/utils/assertUnreachable';

type WalletConnectionStatusUpdatedAction = {
	type: 'wallet-connection-status-updated';
	payload: 'disconnected' | 'connecting' | 'connected';
};

type WalletConnectedAction = {
	type: 'wallet-connected';
	payload: {
		wallet: WalletWithSuiFeatures;
		currentAccount: WalletAccount | null;
	};
};

type WalletDisconnectedAction = {
	type: 'wallet-disconnected';
	payload?: never;
};

type WalletPropertiesChangedAction = {
	type: 'wallet-properties-changed';
	payload: {
		updatedAccounts: readonly WalletAccount[];
		currentAccount: WalletAccount | null;
	};
};

type WalletRegisteredAction = {
	type: 'wallet-registered';
	payload: {
		updatedWallets: WalletWithSuiFeatures[];
	};
};

type WalletUnregisteredAction = {
	type: 'wallet-unregistered';
	payload: {
		updatedWallets: WalletWithSuiFeatures[];
		unregisteredWallet: Wallet;
	};
};

type WalletAccountSwitchedAction = {
	type: 'wallet-account-switched';
	payload: WalletAccount;
};

export type WalletState = {
	wallets: WalletWithSuiFeatures[];
	currentWallet: WalletWithSuiFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	connectionStatus: 'disconnected' | 'connecting' | 'connected';
};

export type WalletAction =
	| WalletConnectionStatusUpdatedAction
	| WalletConnectedAction
	| WalletDisconnectedAction
	| WalletPropertiesChangedAction
	| WalletRegisteredAction
	| WalletUnregisteredAction
	| WalletAccountSwitchedAction;

export function walletReducer(
	walletState: WalletState,
	{ type, payload }: WalletAction,
): WalletState {
	switch (type) {
		case 'wallet-connection-status-updated':
			return {
				...walletState,
				connectionStatus: payload,
			};
		case 'wallet-connected':
			return {
				...walletState,
				currentWallet: payload.wallet,
				accounts: payload.wallet.accounts,
				currentAccount: payload.currentAccount,
				connectionStatus: 'connected',
			};
		case 'wallet-disconnected': {
			return {
				...walletState,
				currentWallet: null,
				accounts: [],
				currentAccount: null,
				connectionStatus: 'disconnected',
			};
		}
		case 'wallet-properties-changed': {
			return {
				...walletState,
				accounts: payload.updatedAccounts,
				currentAccount: payload.currentAccount,
			};
		}
		case 'wallet-registered': {
			return {
				...walletState,
				wallets: payload.updatedWallets,
			};
		}
		case 'wallet-unregistered': {
			if (walletState.currentWallet?.name === payload.unregisteredWallet.name) {
				return {
					...walletState,
					currentWallet: null,
					accounts: [],
					currentAccount: null,
				};
			}
			return {
				...walletState,
				wallets: payload.updatedWallets,
			};
		}
		case 'wallet-account-switched':
			return {
				...walletState,
			};
		default:
			assertUnreachable(type);
	}
}
