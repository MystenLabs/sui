// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletWithSuiFeatures } from '@mysten/wallet-standard';
import { isWalletWithSuiFeatures } from '@mysten/wallet-standard';
import type { StorageAdapter } from './storageAdapters.js';

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
