// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../..//errors/walletErrors.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { MethodTypes } from '../../experimental/store/methods.js';
import { useWalletStore } from './useWalletStore.js';

type Input = MethodTypes['signPersonalMessage']['input'];
type Output = Awaited<MethodTypes['signPersonalMessage']['output']>;

type UseSignPersonalMessageError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignPersonalMessageMutationOptions = Omit<
	UseMutationOptions<Output, UseSignPersonalMessageError, Input, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useSignPersonalMessage({
	mutationKey,
	...mutationOptions
}: UseSignPersonalMessageMutationOptions = {}): UseMutationResult<
	Output,
	UseSignPersonalMessageError,
	Input
> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.signPersonalMessage(mutationKey),
		mutationFn: async (args) => {
			return store.signPersonalMessage(args);
		},
		...mutationOptions,
	});
}
