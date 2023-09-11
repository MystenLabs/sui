// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { useConnectWallet } from '../hooks/wallet/useConnectWallet.js';
import { useWalletContext } from '../hooks/wallet/useWalletContext.js';
import type { StorageAdapter } from '../utils/storageAdapters.js';
import type { WalletAccount, WalletWithRequiredFeatures } from '@mysten/wallet-standard';

type AutoConnectWalletProps = {
	storageKey: string;
	storageAdapter: StorageAdapter;
	children: ReactNode;
};

export function AutoConnectWallet({
	storageKey,
	storageAdapter,
	children,
}: AutoConnectWalletProps) {
	const { wallets, currentWallet: wallet, currentAccount: account } = useWalletContext();
	const [previousWallet, setPreviousWallet] = useState<WalletWithRequiredFeatures | null>(null);
	const [currentWallet, setCurrentWallet] = useState<WalletWithRequiredFeatures | null>(wallet);
	const [previousAccount, setPreviousAccount] = useState<WalletAccount | null>(null);
	const [currentAccount, setCurrentAccount] = useState<WalletAccount | null>(account);
	const { mutate: connectWallet } = useConnectWallet();

	// Instead of abstracting the previous wallet and account state logic into a generic
	// usePrevious hook, we'll write this plainly to make it clear that this code depends
	// on triggering shallow re-renders by updating state in the render code.
	if (wallet !== currentWallet) {
		setPreviousWallet(currentWallet);
		setCurrentWallet(wallet);
	}

	if (account !== currentAccount) {
		setPreviousAccount(currentAccount);
		setCurrentAccount(account);
	}

	useEffect(() => {
		if (wallet !== previousWallet || account !== previousAccount) {
			if (wallet) {
				setWalletConnectionInfo({
					storageAdapter,
					storageKey,
					walletName: wallet.name,
					accountAddress: account?.address,
				});
			} else {
				removeWalletConnectionInfo(storageAdapter, storageKey);
			}
		}
	}, [account, previousAccount, previousWallet, storageAdapter, storageKey, wallet]);

	useEffect(() => {
		(async function autoConnectWallet() {
			const connectionInfo = await getWalletConnectionInfo(storageAdapter, storageKey);
			const { walletName, accountAddress } = connectionInfo || {};
			const wallet = walletName ? wallets.find((wallet) => wallet.name === walletName) : null;

			if (wallet) {
				connectWallet({ wallet, accountAddress, silent: true });
			}
		})();
	}, [connectWallet, storageAdapter, storageKey, wallets]);

	return children;
}

async function getWalletConnectionInfo(storageAdapter: StorageAdapter, storageKey: string) {
	try {
		const connectionInfo = await storageAdapter.get(storageKey);
		return connectionInfo
			? (JSON.parse(connectionInfo) as { walletName: string; accountAddress?: string })
			: null;
	} catch (error) {
		console.warn(
			'[dApp-kit] Error: Failed to retrieve wallet connection info from storage.',
			error,
		);
	}
	return undefined;
}

async function setWalletConnectionInfo({
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
		console.warn('[dApp-kit] Error: Failed to save wallet connection info to storage.', error);
	}
}

async function removeWalletConnectionInfo(storageAdapter: StorageAdapter, storageKey: string) {
	try {
		await storageAdapter.remove(storageKey);
	} catch (error) {
		console.warn('[dApp-kit] Error: Failed to remove wallet connection info from storage.', error);
	}
}
