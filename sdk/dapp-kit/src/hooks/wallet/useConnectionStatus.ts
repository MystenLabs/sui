// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useDAppKitStore } from '../useDAppKitStore.js';

/**
 * Retrieves the
 */
export function useConnectionStatus() {
	return useDAppKitStore((state) => state.connectionStatus);
}
