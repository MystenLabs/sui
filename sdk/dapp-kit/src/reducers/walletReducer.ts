// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount, Wallet, WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { assertUnreachable } from '../utils/assertUnreachable.js';

export type WalletState = {
	wallets: WalletWithRequiredFeatures[];
	currentWallet: WalletWithRequiredFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	connectionStatus: 'disconnected' | 'connecting' | 'connected';
};

type WalletRegisteredAction = {
	type: 'wallet-registered';
	payload: {
		updatedWallets: WalletWithRequiredFeatures[];
	};
};

type WalletUnregisteredAction = {
	type: 'wallet-unregistered';
	payload: {
		updatedWallets: WalletWithRequiredFeatures[];
		unregisteredWallet: Wallet;
	};
};

type WalletConnectionStatusUpdatedAction = {
	type: 'wallet-connection-status-updated';
	payload: WalletState['connectionStatus'];
};

type WalletConnectedAction = {
	type: 'wallet-connected';
	payload: {
		wallet: WalletWithRequiredFeatures;
		currentAccount: WalletAccount | null;
	};
};

type WalletDisconnectedAction = {
	type: 'wallet-disconnected';
	payload?: never;
};

type WalletAccountSwitchedAction = {
	type: 'wallet-account-switched';
	payload: WalletAccount;
};

export type WalletAction =
	| WalletConnectionStatusUpdatedAction
	| WalletConnectedAction
	| WalletDisconnectedAction
	| WalletRegisteredAction
	| WalletUnregisteredAction
	| WalletAccountSwitchedAction;

export function walletReducer(state: WalletState, { type, payload }: WalletAction): WalletState {
	switch (type) {
		case 'wallet-registered': {
			return {
				...state,
				wallets: payload.updatedWallets,
			};
		}
		case 'wallet-unregistered': {
			if (state.currentWallet?.name === payload.unregisteredWallet.name) {
				return {
					...state,
					wallets: payload.updatedWallets,
					currentWallet: null,
					accounts: [],
					currentAccount: null,
					connectionStatus: 'disconnected',
				};
			}
			return {
				...state,
				wallets: payload.updatedWallets,
			};
		}
		case 'wallet-connection-status-updated':
			return {
				...state,
				connectionStatus: payload,
			};
		case 'wallet-connected':
			return {
				...state,
				currentWallet: payload.wallet,
				accounts: payload.wallet.accounts,
				currentAccount: payload.currentAccount,
				connectionStatus: 'connected',
			};
		case 'wallet-disconnected': {
			return {
				...state,
				currentWallet: null,
				accounts: [],
				currentAccount: null,
				connectionStatus: 'disconnected',
			};
		}
		case 'wallet-account-switched':
			return {
				...state,
				currentAccount: payload,
			};
		default:
			assertUnreachable(type);
	}
}
