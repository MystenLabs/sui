// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';

import type { StoreState } from '../../walletStore.js';

/**
 * Retrieves the wallet account that is currently selected, if one exists.
 */
export function getCurrentAccount(state: StoreState): WalletAccount | null {
	return state.currentAccount;
}
