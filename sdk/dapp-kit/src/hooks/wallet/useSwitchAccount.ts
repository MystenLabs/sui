// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { setMostRecentWalletConnectionInfo } from 'dapp-kit/src/components/wallet-provider/walletUtils';
import { WalletNotConnectedError, WalletNotFoundError } from 'dapp-kit/src/errors/walletErrors';

type SwitchAccountArgs = {
	accountAddress: string;
};

type SwitchAccountResult = void;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<SwitchAccountResult, Error, SwitchAccountArgs, unknown>,
	'mutationKey' | 'mutationFn'
>;

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useSwitchAccount({ ...mutationOptions }: UseSwitchAccountMutationOptions) {
	const { storageAdapter, storageKey, accounts, currentWallet, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: mutationKey({ accountAddress }),
		mutationFn: async ({ accountAddress }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const accountToSelect = currentWallet.accounts.find(
				(account) => account.address === accountAddress,
			);
			if (!accountToSelect) {
				// throw some custom error
				throw new Error('');
			}

			dispatch({ type: 'wallet-account-switched', payload: accountAddress });

			await setMostRecentWalletConnectionInfo({
				storageAdapter,
				storageKey,
				walletName: currentWallet.name,
				accountAddress: accountAddress,
			});
		},
		...mutationOptions,
	});
}
