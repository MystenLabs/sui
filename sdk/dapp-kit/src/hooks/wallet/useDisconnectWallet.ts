// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { walletMutationKeys } from 'dapp-kit/src/constants/walletMutationKeys';
import { WalletNotConnectedError } from 'dapp-kit/src/errors/walletErrors';

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

			// Wallets aren't required to implement the disconnect feature, so we'll
			// optionally call the disconnect feature if it exists and reset the UI
			// state on the frontend at a minimum.
			await currentWallet.features['standard:disconnect']?.disconnect();
			dispatch({ type: 'wallet-disconnected' });

			try {
				await storageAdapter.remove(storageKey);
			} catch {
				// Ignore error
			}
		},
		...mutationOptions,
	});
}
