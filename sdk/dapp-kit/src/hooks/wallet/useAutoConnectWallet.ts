// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useLayoutEffect, useState } from 'react';

import { getWalletUniqueIdentifier } from '../../utils/walletUtils.js';
import { useConnectWallet } from './useConnectWallet.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

export function useAutoConnectWallet(): 'disabled' | 'idle' | 'attempted' {
	const { mutateAsync: connectWallet } = useConnectWallet();
	const autoConnectEnabled = useWalletStore((state) => state.autoConnectEnabled);
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);
	const wallets = useWallets();
	const { isConnected } = useCurrentWallet();

	const [clientOnly, setClientOnly] = useState(false);
	useLayoutEffect(() => {
		setClientOnly(true);
	}, []);

	const { data, isError } = useQuery({
		queryKey: [
			'@mysten/dapp-kit',
			'autoconnect',
			{
				isConnected,
				autoConnectEnabled,
				lastConnectedWalletName,
				lastConnectedAccountAddress,
				walletCount: wallets.length,
			},
		],
		queryFn: async () => {
			if (!autoConnectEnabled) {
				return 'disabled';
			}

			if (!lastConnectedWalletName || !lastConnectedAccountAddress || isConnected) {
				return 'attempted';
			}

			const wallet = wallets.find(
				(wallet) => getWalletUniqueIdentifier(wallet) === lastConnectedWalletName,
			);
			if (wallet) {
				await connectWallet({
					wallet,
					accountAddress: lastConnectedAccountAddress,
					silent: true,
				});
			}

			return 'attempted';
		},
		enabled: autoConnectEnabled,
		persister: undefined,
		gcTime: 0,
		staleTime: 0,
		networkMode: 'always',
		retry: false,
		retryOnMount: false,
		refetchInterval: false,
		refetchIntervalInBackground: false,
		refetchOnMount: false,
		refetchOnReconnect: false,
		refetchOnWindowFocus: false,
	});

	if (!autoConnectEnabled) {
		return 'disabled';
	}

	// We always initialize with "idle" so that in SSR environments, we guarantee that the initial render states always agree:
	if (!clientOnly) {
		return 'idle';
	}

	if (isConnected) {
		return 'attempted';
	}

	if (!lastConnectedWalletName) {
		return 'attempted';
	}

	return isError ? 'attempted' : data ?? 'idle';
}
