// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

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
	const { isDisconnected } = useCurrentWallet();

	const { data, isError } = useQuery({
		queryKey: [
			'@mysten/dapp-kit',
			'autoconnect',
			{
				isDisconnected,
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

			if (!lastConnectedWalletName || !lastConnectedAccountAddress || !isDisconnected) {
				return 'attempted';
			}

			const wallet = wallets.find((wallet) => wallet.name === lastConnectedWalletName);
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

	if (!lastConnectedWalletName) {
		return 'attempted';
	}

	return isError ? 'attempted' : data ?? 'idle';
}
