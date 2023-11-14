// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletWithRequiredFeatures } from '@mysten/wallet-standard';
import { useMutation } from '@tanstack/react-query';
import { useLayoutEffect } from 'react';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { useConnectWallet } from './useConnectWallet.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWallets } from './useWallets.js';
import { useWalletStore } from './useWalletStore.js';

export function useAutoConnectWallet() {
	const { mutateAsync: connectWallet } = useConnectWallet();
	const autoConnectEnabled = useWalletStore((state) => state.autoConnectEnabled);
	const lastConnectedWalletName = useWalletStore((state) => state.lastConnectedWalletName);
	const lastConnectedAccountAddress = useWalletStore((state) => state.lastConnectedAccountAddress);
	const wallets = useWallets();
	const { isDisconnected } = useCurrentWallet();

	const { mutate } = useMutation({
		mutationKey: walletMutationKeys.autoconnectWallet(),
		mutationFn: async ({
			autoConnectEnabled,
			lastConnectedWalletName,
			lastConnectedAccountAddress,
		}: {
			wallets: WalletWithRequiredFeatures[];
			autoConnectEnabled: boolean;
			lastConnectedWalletName: string | null;
			lastConnectedAccountAddress: string | null;
			isDisconnected: boolean;
		}) => {
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
	});

	useLayoutEffect(() => {
		if (autoConnectEnabled) {
			mutate({
				autoConnectEnabled,
				isDisconnected,
				wallets,
				lastConnectedAccountAddress,
				lastConnectedWalletName,
			});
		}
	}, [
		mutate,
		autoConnectEnabled,
		isDisconnected,
		wallets,
		lastConnectedAccountAddress,
		lastConnectedWalletName,
	]);
}
