// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the
 */
export function useConnectionStatus() {
	return useWalletStore((state) => state.connectionStatus);
}
