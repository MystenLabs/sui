// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignTransactionBlockInput } from '@mysten/wallet-standard';
import type { SuiSignTransactionBlockOutput } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type UseSignTransactionBlockArgs = SuiSignTransactionBlockInput;
type UseSignTransactionBlockResult = SuiSignTransactionBlockOutput;

type UseSignTransactionBlockMutationOptions = Omit<
	UseMutationOptions<UseSignTransactionBlockResult, Error, UseSignTransactionBlockArgs, unknown>,
	'mutationKey' | 'mutationFn'
>;

// TODO: Figure out the query/mutation key story and whether or not we want to expose
// key factories from dapp-kit
function mutationKey(args: Partial<UseSignTransactionBlockArgs>) {
	return [{ scope: 'wallet', entity: 'sign-transaction-block', ...args }] as const;
}

/**
 * Mutation hook for prompting the user to sign a transaction block.
 */
export function useSignTransactionBlock({
	account,
	chain,
	transactionBlock,
	...mutationOptions
}: Partial<UseSignTransactionBlockArgs> & UseSignTransactionBlockMutationOptions) {
	const { currentWallet } = useWalletContext();

	return useMutation({
		mutationKey: mutationKey({ account, chain, transactionBlock }),
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
