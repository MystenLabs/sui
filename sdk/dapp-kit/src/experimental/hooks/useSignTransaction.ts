// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { MethodTypes } from '../../experimental/store/methods.js';
import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import { useWalletStore } from './useWalletStore.js';

type Input = MethodTypes['signTransaction']['input'];
type Output = Awaited<MethodTypes['signTransaction']['output']>;

type UseSignTransactionError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignTransactionMutationOptions = Omit<
	UseMutationOptions<Output, UseSignTransactionError, Input, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a transaction.
 */
export function useSignTransaction({
	mutationKey,
	...mutationOptions
}: UseSignTransactionMutationOptions = {}): UseMutationResult<
	Output,
	UseSignTransactionError,
	Input
> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.signTransaction(mutationKey),
		mutationFn: async (args) => {
			return store.signTransaction(args);
		},
		...mutationOptions,
	});
}
