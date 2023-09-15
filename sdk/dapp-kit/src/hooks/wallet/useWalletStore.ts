// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletContext } from '../../contexts/walletContext.js';
import type { StoreState } from '../../walletStore.js';
import { useContext } from 'react';
import { useStore } from 'zustand';

export function useWalletStore<T>(selector: (state: StoreState) => T): T {
	const store = useContext(WalletContext);
	if (!store) {
		throw new Error(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	}
	return useStore(store, selector);
}
