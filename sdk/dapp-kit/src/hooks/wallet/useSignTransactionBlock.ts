// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignTransactionBlockInput } from '@mysten/wallet-standard';
import type { SuiSignTransactionBlockOutput } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { walletMutationKeys } from 'dapp-kit/src/constants/walletMutationKeys';
import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type UseSignTransactionBlockArgs = SuiSignTransactionBlockInput;
type UseSignTransactionBlockResult = SuiSignTransactionBlockOutput;

type UseSignTransactionBlockMutationOptions = Omit<
	UseMutationOptions<UseSignTransactionBlockResult, Error, UseSignTransactionBlockArgs, unknown>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a transaction block.
 */
export function useSignTransactionBlock({
	mutationKey,
	...mutationOptions
}: UseSignTransactionBlockMutationOptions = {}) {
	const { currentWallet } = useWalletContext();

	return useMutation({
		mutationKey: walletMutationKeys.signTransactionBlock(mutationKey),
		mutationFn: async (signTransactionBlockInput) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signTransactionBlockFeature = currentWallet.features['sui:signTransactionBlock'];
			if (!signTransactionBlockFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signTransactionBlock` feature.",
				);
			}

			return await signTransactionBlockFeature.signTransactionBlock({
				...signTransactionBlockInput,
				account: signTransactionBlockInput.account,
			});
		},
		...mutationOptions,
	});
}
