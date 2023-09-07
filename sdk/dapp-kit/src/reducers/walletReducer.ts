// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithSuiFeatures, WalletAccount, Wallet } from '@mysten/wallet-standard';
import { assertUnreachable } from '../utils/assertUnreachable.js';

export type WalletState = {
	wallets: WalletWithSuiFeatures[];
	currentWallet: WalletWithSuiFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	connectionStatus: 'disconnected' | 'connecting' | 'connected';
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

export type WalletAction = WalletRegisteredAction | WalletUnregisteredAction;

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
		default:
			assertUnreachable(type);
	}
}
