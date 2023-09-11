// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	MinimallyRequiredFeatures,
	Wallet,
	WalletWithFeatures,
} from '@mysten/wallet-standard';
import { isWalletWithRequiredFeatureSet } from '@mysten/wallet-standard';
import type { StorageAdapter } from './storageAdapters.js';

export function sortWallets<AdditionalFeatures extends Wallet['features']>(
	wallets: readonly Wallet[],
	preferredWallets: string[],
	requiredFeatures?: (keyof AdditionalFeatures)[],
) {
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

export async function setMostRecentWalletConnectionInfo({
	storageAdapter,
	storageKey,
	walletName,
	accountAddress,
}: {
	storageAdapter: StorageAdapter;
	storageKey: string;
	walletName: string;
	accountAddress?: string;
}) {
	try {
		await storageAdapter.set(storageKey, JSON.stringify({ walletName, accountAddress }));
	} catch (error) {
		// We'll skip error handling here and just report the error to the console since persisting connection
		// info isn't essential functionality and storage adapters can be plugged in by the consumer.
		console.warn('[dApp-kit] Error: Failed to save wallet connection info to storage.', error);
	}
}
