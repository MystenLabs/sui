// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getDisconnectWallet } from '../../core/wallet/getDisconnectWallet.js';
import type { WalletNotConnectedError } from '../../errors/walletErrors.js';
import { useWalletStore } from './useWalletStore.js';

type UseDisconnectWalletError = WalletNotConnectedError | Error;

type UseDisconnectWalletMutationOptions = Omit<
	UseMutationOptions<void, UseDisconnectWalletError, void, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for disconnecting from an active wallet connection, if currently connected.
 */
export function useDisconnectWallet({
	mutationKey,
	...mutationOptions
}: UseDisconnectWalletMutationOptions = {}): UseMutationResult<
	void,
	UseDisconnectWalletError,
	void
> {
	const disconnectWallet = useWalletStore(getDisconnectWallet);

	return useMutation({
		mutationKey: walletMutationKeys.disconnectWallet(mutationKey),
		mutationFn: async () => {
			return disconnectWallet();
		},
		...mutationOptions,
	});
}
