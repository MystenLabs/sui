// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiSignAndExecuteTransactionBlockInput } from '@mysten/wallet-standard';
import type { SuiSignAndExecuteTransactionBlockOutput } from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import {
	WalletFeatureNotSupportedError,
	WalletNotConnectedError,
} from 'dapp-kit/src/errors/walletErrors';

type UseSignAndExecuteTransactionBlockArgs = SuiSignAndExecuteTransactionBlockInput;
type UseSignAndExecuteTransactionBlockResult = SuiSignAndExecuteTransactionBlockOutput;

type UseSignAndExecuteTransactionBlockMutationOptions = Omit<
	UseMutationOptions<
		UseSignAndExecuteTransactionBlockResult,
		Error,
		UseSignAndExecuteTransactionBlockArgs,
		unknown
	>,
	'mutationKey' | 'mutationFn'
>;

// TODO: Figure out the query/mutation key story and whether or not we want to expose
// key factories from dapp-kit
function mutationKey(args: UseSignAndExecuteTransactionBlockArgs) {
	return [{ scope: 'wallet', entity: 'sign-and-execute-transaction-block', ...args }] as const;
}

/**
 * Mutation hook for prompting the user to sign and execute a transaction block.
 */
export function useSignAndExecuteTransactionBlock({
	account,
	chain,
	transactionBlock,
	...mutationOptions
}: UseSignAndExecuteTransactionBlockArgs & UseSignAndExecuteTransactionBlockMutationOptions) {
	const { currentWallet } = useWalletContext();

	return useMutation({
		mutationKey: mutationKey({ account, chain, transactionBlock }),
		mutationFn: async (signAndExecuteTransactionBlockInput) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signAndExecuteTransactionBlockFeature =
				currentWallet.features['sui:signAndExecuteTransactionBlock'];
			if (!signAndExecuteTransactionBlockFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signAndExecuteTransactionBlock` feature.",
				);
			}

			return await signAndExecuteTransactionBlockFeature.signAndExecuteTransactionBlock({
				...signAndExecuteTransactionBlockInput,
				account: signAndExecuteTransactionBlockInput.account,
			});
		},
		...mutationOptions,
	});
}
