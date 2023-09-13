// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DAppKitContext } from 'dapp-kit/src/contexts/dAppKitContext';
import type { StoreState } from 'dapp-kit/src/store.js';
import { useContext } from 'react';
import { useStore } from 'zustand';

export function useDAppKitStore<T>(selector: (state: StoreState) => T): T {
	const store = useContext(DAppKitContext);
	if (!store) {
		throw new Error(
			'Could not find DAppKitContext. Ensure that you have set up the DAppKitProvider.',
		);
	}
	return useStore(store, selector);
}
