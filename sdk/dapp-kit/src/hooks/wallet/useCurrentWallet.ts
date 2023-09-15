// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the wallet that is currently connected to the dApp, if one exists.
 */
export function useCurrentWallet() {
	return useWalletStore((state) => state.currentWallet);
}
