// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';

import { SuiClientProvider } from '../components/SuiClientProvider.js';
import type { DappKitStore } from './store/index.js';
import { DappKitStoreContext } from './storeContext.js';

interface DappKitProviderProps {
	store: DappKitStore;
	children: ReactNode;
}

export function DappKitProvider({ store, children }: DappKitProviderProps) {
	// NOTE: This currently _also_ renders the legacy `SuiClientProvider` so that the existing hooks work,
	// but once we stabalize this API we'll migrate those APIs to use the new store.
	return (
		<DappKitStoreContext.Provider value={store}>
			<SuiClientProvider>{children}</SuiClientProvider>
		</DappKitStoreContext.Provider>
	);
}
