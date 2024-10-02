// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { WalletAccountNotFoundError, WalletNotConnectedError } from '../../errors/walletErrors.js';
import { useCurrentWallet } from './useCurrentWallet.js';
import { useWalletStore } from './useWalletStore.js';

type SwitchAccountArgs = {
	account: WalletAccount;
};

type SwitchAccountResult = void;

type UseSwitchAccountError = WalletNotConnectedError | WalletAccountNotFoundError | Error;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<SwitchAccountResult, UseSwitchAccountError, SwitchAccountArgs, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for switching to a specific wallet account.
 */
export function useSwitchAccount({
	mutationKey,
	...mutationOptions
}: UseSwitchAccountMutationOptions = {}): UseMutationResult<
	SwitchAccountResult,
	UseSwitchAccountError,
	SwitchAccountArgs
> {
	const { currentWallet } = useCurrentWallet();
	const setAccountSwitched = useWalletStore((state) => state.setAccountSwitched);

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

			setAccountSwitched(accountToSelect);
		},
		...mutationOptions,
	});
}
