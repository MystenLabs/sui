// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useDAppKitStore } from '../useDAppKitStore.js';

/**
 * Retrieves a list of registered wallets available to the dApp sorted by preference.
 */
export function useWallets() {
	return useDAppKitStore((state) => state.wallets);
}
