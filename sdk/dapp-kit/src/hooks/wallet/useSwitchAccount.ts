// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from '../../components/WalletProvider.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { WalletAccountNotFoundError, WalletNotConnectedError } from '../../errors/walletErrors.js';

type SwitchAccountArgs = {
	account: WalletAccount;
};

type SwitchAccountResult = void;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<
		SwitchAccountResult,
		WalletNotConnectedError | WalletAccountNotFoundError | Error,
		SwitchAccountArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for switching to a specific wallet account.
 */
export function useSwitchAccount({
	mutationKey,
	...mutationOptions
}: UseSwitchAccountMutationOptions = {}) {
	const { currentWallet, dispatch } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.switchAccount(mutationKey),
		mutationFn: async ({ account }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const accountToSelect = currentWallet.accounts.find(
				(walletAccount) => walletAccount.address === account.address,
			);
			if (!accountToSelect) {
				throw new WalletAccountNotFoundError(
					`No account with address ${account.address} is connected to ${currentWallet.name}.`,
				);
			}

			dispatch({ type: 'wallet-account-switched', payload: accountToSelect });
		},
		...mutationOptions,
	});
}
