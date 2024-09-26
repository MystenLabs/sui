// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	StandardConnectInput,
	StandardConnectOutput,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getConnectWallet } from '../../core/wallet/getConnectWallet.js';
import { useWalletStore } from './useWalletStore.js';

type ConnectWalletArgs = {
	/** The wallet to connect to. */
	wallet: WalletWithRequiredFeatures;

	/** An optional account address to connect to. Defaults to the first authorized account. */
	accountAddress?: string;
} & StandardConnectInput;

type ConnectWalletResult = StandardConnectOutput;

type UseConnectWalletMutationOptions = Omit<
	UseMutationOptions<ConnectWalletResult, Error, ConnectWalletArgs, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for establishing a connection to a specific wallet.
 */
export function useConnectWallet({
	mutationKey,
	...mutationOptions
}: UseConnectWalletMutationOptions = {}): UseMutationResult<
	ConnectWalletResult,
	Error,
	ConnectWalletArgs,
	unknown
> {
	const connectWallet = useWalletStore(getConnectWallet);

	return useMutation({
		mutationKey: walletMutationKeys.connectWallet(mutationKey),
		mutationFn: async (args) => {
			return connectWallet(args);
		},
		...mutationOptions,
	});
}
