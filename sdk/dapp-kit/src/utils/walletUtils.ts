// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	MinimallyRequiredFeatures,
	Wallet,
	WalletWithFeatures,
} from '@mysten/wallet-standard';
import { getNormalizedSuiWallets } from '@mysten/wallet-standard';

export function getRegisteredWallets<AdditionalFeatures extends Wallet['features']>(
	preferredWallets: string[],
	requiredFeatures: (keyof AdditionalFeatures & keyof Wallet['features'])[] = [],
) {
	const suiWallets = getNormalizedSuiWallets<
		keyof AdditionalFeatures & keyof Wallet['features'],
		AdditionalFeatures
	>(requiredFeatures) as WalletWithFeatures<MinimallyRequiredFeatures & AdditionalFeatures>[];

	return [
		// Preferred wallets, in order:
		...preferredWallets
			.map((name) => suiWallets.find((wallet) => wallet.name === name)!)
			.filter(Boolean),

		// Wallets in default order:
		...suiWallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
	];
}

export function getWalletUniqueIdentifier(wallet?: Wallet) {
	return wallet?.id ?? wallet?.name;
}
