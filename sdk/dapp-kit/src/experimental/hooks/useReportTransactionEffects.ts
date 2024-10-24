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

type Input = MethodTypes['reportTransactionEffects']['input'];
type Output = Awaited<MethodTypes['reportTransactionEffects']['output']>;

type UseReportTransactionEffectsError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseReportTransactionEffectsMutationOptions = Omit<
	UseMutationOptions<Output, UseReportTransactionEffectsError, Input, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useReportTransactionEffects({
	mutationKey,
	...mutationOptions
}: UseReportTransactionEffectsMutationOptions = {}): UseMutationResult<
	Output,
	UseReportTransactionEffectsError,
	Input
> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.reportTransactionEffects(mutationKey),
		mutationFn: async (args) => {
			return store.reportTransactionEffects(args);
		},
		...mutationOptions,
	});
}
