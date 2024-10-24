// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { MethodTypes } from '../../experimental/store/methods.js';
import { useWalletStore } from './useWalletStore.js';

type Input = MethodTypes['connectWallet']['input'];
type Output = Awaited<MethodTypes['connectWallet']['output']>;

type UseConnectWalletMutationOptions = Omit<
	UseMutationOptions<Output, Error, Input, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useConnectWallet({
	mutationKey,
	...mutationOptions
}: UseConnectWalletMutationOptions = {}): UseMutationResult<Output, Error, Input, unknown> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async (args) => {
			return store.connectWallet(args);
		},
		...mutationOptions,
	});
}
