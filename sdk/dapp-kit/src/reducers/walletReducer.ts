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
