// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet, WalletWithSuiFeatures } from '@mysten/wallet-standard';
import { isWalletWithSuiFeatures } from '@mysten/wallet-standard';
import type { StorageAdapter } from 'dapp-kit/src/utils/storageAdapters';

export type WalletAccountStorageKey = `${string}-${string}`;

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
	accountAddress: string;
}) {
	try {
		await storageAdapter.set(storageKey, `${walletName}-${accountAddress}`);
	} catch {
		// Ignore error
	}
}

export async function getMostRecentWalletConnectionInfo(
	storageAdapter: StorageAdapter,
	storageKey: string,
) {
	try {
		const lastWalletConnectionInfo = await storageAdapter.get(storageKey);
		if (lastWalletConnectionInfo) {
			const [walletName, accountAddress] = lastWalletConnectionInfo.split('-');
			return {
				walletName,
				accountAddress,
			};
		}
	} catch {
		// Ignore error
	}
	return {};
}
