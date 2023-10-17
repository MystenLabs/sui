// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the wallet auto-connection process.
 */
export function useAutoConnectStatus() {
	return useWalletStore((state) => state.autoConnectionStatus);
}
