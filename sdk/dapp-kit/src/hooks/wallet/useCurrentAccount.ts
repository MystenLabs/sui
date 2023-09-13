// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useDAppKitStore } from '../useDAppKitStore.js';

/**
 * Retrieves the wallet account that is currently selected, if one exists.
 */
export function useCurrentAccount() {
	return useDAppKitStore((state) => state.currentAccount);
}
