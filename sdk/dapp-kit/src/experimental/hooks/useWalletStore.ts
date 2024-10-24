// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useContext } from 'react';

import { DappKitStoreContext } from '../storeContext.js';

export function useWalletStore() {
	const store = useContext(DappKitStoreContext);

	if (!store) {
		throw new Error(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	}

	return store;
}
