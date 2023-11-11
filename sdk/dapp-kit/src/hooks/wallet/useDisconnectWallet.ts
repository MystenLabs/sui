// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { WalletNotConnectedError } from '../../errors/walletErrors.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWalletStore } from './useWalletStore.js';

type UseDisconnectWalletError = WalletNotConnectedError | Error;

type UseDisconnectWalletMutationOptions = Omit<
	UseMutationOptions<void, UseDisconnectWalletError, void, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for disconnecting from an active wallet connection, if currently connected.
 */
export function useDisconnectWallet({
	mutationKey,
	...mutationOptions
}: UseDisconnectWalletMutationOptions = {}): UseMutationResult<
	void,
	UseDisconnectWalletError,
	void
> {
	const { currentWallet } = useCurrentWallet();
	const setWalletDisconnected = useWalletStore((state) => state.setWalletDisconnected);

	return useMutation({
		mutationKey: walletMutationKeys.disconnectWallet(mutationKey),
		mutationFn: async () => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			try {
				// Wallets aren't required to implement the disconnect feature, so we'll
				// optionally call the disconnect feature if it exists and reset the UI
				// state on the frontend at a minimum.
				await currentWallet.features['standard:disconnect']?.disconnect();
			} catch (error) {
				console.error('Failed to disconnect the application from the current wallet.', error);
			}

			setWalletDisconnected();
		},
		...mutationOptions,
	});
}
