// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { WalletContext } from '../../contexts/WalletContext.js';
import { useContext } from 'react';

export function useWalletContext() {
	const context = useContext(WalletContext);
	if (!context) {
		throw new Error(
			'Could not find WalletContext. Ensure that you have set up the WalletProvider.',
		);
	}
	return context;
}
