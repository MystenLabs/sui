// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { MethodTypes } from '../../experimental/store/methods.js';
import type {
	WalletAccountNotFoundError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import { useWalletStore } from './useWalletStore.js';

type Input = MethodTypes['switchAccount']['input'];
type Output = MethodTypes['switchAccount']['output'];

type UseSwitchAccountError = WalletNotConnectedError | WalletAccountNotFoundError | Error;

type UseSwitchAccountMutationOptions = Omit<
	UseMutationOptions<Output, UseSwitchAccountError, Input, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for switching to a specific wallet account.
 */
export function useSwitchAccount({
	mutationKey,
	...mutationOptions
}: UseSwitchAccountMutationOptions = {}): UseMutationResult<Output, UseSwitchAccountError, Input> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.switchAccount(mutationKey),
		mutationFn: async (args) => {
			return store.switchAccount(args);
		},
		...mutationOptions,
	});
}
