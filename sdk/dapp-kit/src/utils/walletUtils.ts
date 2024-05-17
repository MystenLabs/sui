// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	MinimallyRequiredFeatures,
	Wallet,
	WalletWithFeatures,
} from '@mysten/wallet-standard';
import { getWallets, isWalletWithRequiredFeatureSet } from '@mysten/wallet-standard';

export function getRegisteredWallets<AdditionalFeatures extends Wallet['features']>(
	preferredWallets: string[],
	requiredFeatures?: (keyof AdditionalFeatures)[],
) {
	const walletsApi = getWallets();
	const wallets = walletsApi.get();

	const suiWallets = wallets.filter(
		(wallet): wallet is WalletWithFeatures<MinimallyRequiredFeatures & AdditionalFeatures> =>
			isWalletWithRequiredFeatureSet(wallet, requiredFeatures),
	);

	return [
		// Preferred wallets, in order:
		...(preferredWallets
			.map((name) => suiWallets.find((wallet) => wallet.name === name))
			.filter(Boolean) as WalletWithFeatures<MinimallyRequiredFeatures & AdditionalFeatures>[]),

		// Wallets in default order:
		...suiWallets.filter((wallet) => !preferredWallets.includes(wallet.name)),
	];
}

export function getWalletUniqueIdentifier(wallet?: Wallet) {
	return wallet?.id ?? wallet?.name;
}
