// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useConnectWallet } from './useConnectWallet.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

export function useAutoConnectWallet(autoConnectEnabled: boolean) {
	const { mutateAsync: connectWallet } = useConnectWallet();
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);
	const wallets = useWallets();
	const { isDisconnected } = useCurrentWallet();

	useQuery({
		queryKey: [
			'@mysten/dapp-kit',
			'autoconnect',
			{
				autoConnectEnabled,
				lastConnectedWalletName,
				lastConnectedAccountAddress,
				isDisconnected,
				wallets: wallets.length,
			},
		],
		queryFn: async () => {
			if (
				!autoConnectEnabled ||
				!lastConnectedWalletName ||
				!lastConnectedAccountAddress ||
				!isDisconnected
			) {
				return 'not-attempted';
			}

			const wallet = wallets.find((wallet) => wallet.name === lastConnectedWalletName);
			if (wallet) {
				await connectWallet({
					wallet,
					accountAddress: lastConnectedAccountAddress,
					silent: true,
				});
				return 'connected';
			}

			return 'wallet-not-found';
		},
		// NOTE: We don't take in every condition here so that this query always runs if autoConnectEnabled is true, which lets us read the state of this predictably.
		enabled: autoConnectEnabled,
		gcTime: Infinity,
		staleTime: Infinity,
		retry: false,
		refetchInterval: false,
		refetchIntervalInBackground: false,
		refetchOnWindowFocus: false,
		refetchOnReconnect: false,
	});
}
