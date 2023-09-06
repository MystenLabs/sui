// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { setMostRecentWalletConnectionInfo } from 'dapp-kit/src/components/wallet-provider/walletUtils';
import { walletMutationKeys } from 'dapp-kit/src/constants/walletMutationKeys';
import {
	WalletAccountNotFoundError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type SwitchAccountArgs = {
	accountAddress: string;
};

type SwitchAccountResult = void;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<SwitchAccountResult, Error, SwitchAccountArgs, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for switching to a specific wallet account.
 */
export function useSwitchAccount({
	mutationKey,
	...mutationOptions
}: UseSwitchAccountMutationOptions) {
	const { storageAdapter, storageKey, currentWallet, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.switchAccount(mutationKey),
		mutationFn: async ({ accountAddress }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const accountToSelect = currentWallet.accounts.find(
				(account) => account.address === accountAddress,
			);
			if (!accountToSelect) {
				throw new WalletAccountNotFoundError(
					`Failed to switch to account with address ${accountAddress}.`,
				);
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
