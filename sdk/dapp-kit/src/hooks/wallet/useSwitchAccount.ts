// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { WalletAccount } from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getSwitchAccount } from '../../core/wallet/getSwitchAccount.js';
import type {
	WalletAccountNotFoundError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
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
	const switchAccount = useWalletStore(getSwitchAccount);

	return useMutation({
		mutationKey: walletMutationKeys.switchAccount(mutationKey),
		mutationFn: async (args) => {
			return switchAccount(args);
		},
		...mutationOptions,
	});
}
