// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { WalletNotConnectedError } from 'dapp-kit/src/errors/walletErrors';

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
export function useDisconnectWallet(mutationOptions?: UseDisconnectWalletMutationOptions) {
	const { currentWallet, storageAdapter, storageKey, dispatch } = useWalletContext();

	return useMutation({
		mutationKey,
		mutationFn: async () => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			// Wallets aren't required to implement the disconnect feature, so we'll
			// reset the wallet state on the dApp side instead of throwing an error.
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
