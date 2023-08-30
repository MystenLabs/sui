// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type UseDisconnectWalletMutationOptions = Omit<
	UseMutationOptions<void, Error, void, unknown>,
	'mutationKey' | 'mutationFn'
>;

// TODO: Figure out the query/mutation key story and whether or not we want to expose
// key factories from dapp-kit
const mutationKey = [{ scope: 'wallet', entity: 'disconnect-wallet' }] as const;

/**
 * Mutation hook for disconnecting from an active wallet connection, if currently connected.
 */
export function useDisconnectWallet(mutationOptions: UseDisconnectWalletMutationOptions) {
	const { currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey,
		mutationFn: async () => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const disconnectFeature = currentWallet.features['standard:disconnect'];
			if (!disconnectFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `disconnect` feature.",
				);
			}

			await disconnectFeature.disconnect();
			dispatch({ type: 'wallet-disconnected', payload: undefined });

			try {
				await storageAdapter.remove(storageKey);
			} catch {
				/* ignore error */
			}
		},
		...mutationOptions,
	});
}
