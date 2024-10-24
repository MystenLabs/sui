// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useStore } from '@nanostores/react';

import { useWalletStore } from './useWalletStore.js';

/**
 * Retrieves the wallet account that is currently selected, if one exists.
 */
export function useCurrentAccount() {
	const store = useWalletStore();
	return useStore(store.atoms.$currentAccount);
}
