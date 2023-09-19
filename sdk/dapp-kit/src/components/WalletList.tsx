// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallets } from '../hooks/wallet/useWallets.js';

export function WalletList() {
	const wallets = useWallets();

	return wallets.length > 0 ? (
		<ul>
			{wallets.map((wallet) => (
				<li key={wallet.name}></li>
			))}
		</ul>
	) : (
		<div>place</div>
	);
}
