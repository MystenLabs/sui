// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves a list of connected accounts authorized by the dApp.
 */
export function useAccounts() {
	return useWalletStore((state) => state.accounts);
}
