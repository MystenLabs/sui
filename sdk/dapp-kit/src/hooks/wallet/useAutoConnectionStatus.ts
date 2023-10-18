// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the status for the initial wallet auto-connection process.
 */
export function useAutoConnectionStatus() {
	// TODO: Replace this with shareable mutation state once we require react-query v5
	return useWalletStore((state) => state.autoConnectionStatus);
}
