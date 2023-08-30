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
 * 
 * We haven't taken
 * Most wallets in the Sui ecosystem don't currently implement the disconnect feature for
 * historical reasons (we haven't taken a firm stance on whether or not disconnecting
 * your wallet from a dApp should a)
 */
export function useDisconnectWallet(mutationOptions: UseDisconnectWalletMutationOptions) {
	const { currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey,
		mutationFn: async () => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			// TODO: Once we decide to
			const disconnectFeature = currentWallet.features['standard:disconnect'];
			await disconnectFeature?.disconnect();

			dispatch({ type: 'wallet-disconnected' });

			try {
				await storageAdapter.remove(storageKey);
			} catch {
				/* ignore error */
			}
		},
		...mutationOptions,
	});
}
