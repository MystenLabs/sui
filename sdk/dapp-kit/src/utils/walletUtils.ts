// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletWithSuiFeatures } from '@mysten/wallet-standard';
import { isWalletWithSuiFeatures } from '@mysten/wallet-standard';

export function sortWallets(
	wallets: readonly Wallet[],
	preferredWallets: string[],
	requiredFeatures?: string[],
): WalletWithSuiFeatures[] {
	const suiWallets = wallets.filter((wallet): wallet is WalletWithSuiFeatures =>
		isWalletWithSuiFeatures(wallet, requiredFeatures),
	);

	return [
		// Preferred wallets, in order:
		...(preferredWallets
			.map((name) => suiWallets.find((wallet) => wallet.name === name))
			.filter(Boolean) as WalletWithSuiFeatures[]),

		// Wallets in default order:
		...suiWallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
	];
}
