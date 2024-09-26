// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';

import type { StoreState } from '../../walletStore.js';

/**
 * Retrieves a list of connected accounts authorized by the dApp.
 */
export function getAccounts(state: StoreState): readonly WalletAccount[] {
	return state.accounts;
}
