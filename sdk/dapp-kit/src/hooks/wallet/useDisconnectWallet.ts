// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from '../../components/WalletProvider.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { WalletNotConnectedError } from '../../errors/walletErrors.js';

type UseDisconnectWalletMutationOptions = Omit<
	UseMutationOptions<void, Error, void, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for disconnecting from an active wallet connection, if currently connected.
 */
export function useDisconnectWallet({
	mutationKey,
	...mutationOptions
}: UseDisconnectWalletMutationOptions = {}) {
	const { currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

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

			dispatch({ type: 'wallet-disconnected' });

			try {
				await storageAdapter.remove(storageKey);
			} catch (error) {
				// We'll skip error handling here and just report the error to the console since deleting connection
				// info isn't essential functionality and storage adapters can be plugged in by the consumer.
				console.error(
					'[dApp-kit] Error: Failed to remove wallet connection info from storage.',
					error,
				);
			}
		},
		...mutationOptions,
	});
}
