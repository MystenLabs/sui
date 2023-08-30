// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithSuiFeatures, WalletAccount } from '@mysten/wallet-standard';
import { assertUnreachable } from 'dapp-kit/src/utils/assertUnreachable';

export type WalletState = {
	wallets: WalletWithSuiFeatures[];
	currentWallet: WalletWithSuiFeatures | null;
	accounts: readonly WalletAccount[];
	currentAccount: WalletAccount | null;
	status: 'disconnected' | 'connecting' | 'connected' | 'error';
};

export type WalletAction =
	| { type: 'wallet-connected'; payload: WalletWithSuiFeatures }
	| { type: 'wallet-disconnected'; payload?: never }
	| { type: 'wallet-properties-changed'; payload: { updatedAccounts: WalletAccount[] } }
	| { type: 'update-status'; payload: WalletState['status'] };

export function walletReducer(
	walletState: WalletState,
	{ type, payload }: WalletAction,
): WalletState {
	switch (type) {
		case 'wallet-connected':
			return {
				...walletState,
				currentWallet: payload,
				accounts: payload.accounts,
				currentAccount: payload.accounts[0] ?? null,
				status: 'connected',
			};
		case 'wallet-disconnected': {
			return {
				wallets: [],
				currentWallet: null,
				accounts: [],
				currentAccount: null,
				status: 'disconnected',
			};
		}
		case 'wallet-properties-changed': {
			return {
				...walletState,
				accounts: [],
				currentAccount: null,
			};
		}
		case 'update-status':
			return {
				...walletState,
				status: payload,
			};
		default:
			assertUnreachable(type);
	}
}
