// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { StoreState } from '../../walletStore.js';

/**
 * Retrieves a list of registered wallets available to the dApp sorted by preference.
 */
export function getWallets(state: StoreState) {
	return state.wallets;
}
