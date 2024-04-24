// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionBlock } from '@mysten/sui.js/transactions';
import type {
	SuiSignTransactionBlockV2Input,
	SuiSignTransactionBlockV2Output,
} from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../..//errors/walletErrors.js';
import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useSuiClient } from '../useSuiClient.js';
import { useCurrentAccount } from './useCurrentAccount.js';
import { useCurrentWallet } from './useCurrentWallet.js';

type UseSignTransactionBlockArgs = PartialBy<
	Omit<SuiSignTransactionBlockV2Input, 'transactionBlock'>,
	'account' | 'chain'
> & {
	transactionBlock: TransactionBlock;
};

type UseSignTransactionBlockResult = SuiSignTransactionBlockV2Output;

type UseSignTransactionBlockError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignTransactionBlockMutationOptions = Omit<
	UseMutationOptions<
		UseSignTransactionBlockResult,
		UseSignTransactionBlockError,
		UseSignTransactionBlockArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a transaction block.
 */
export function useSignTransactionBlock({
	mutationKey,
	...mutationOptions
}: UseSignTransactionBlockMutationOptions = {}): UseMutationResult<
	UseSignTransactionBlockResult,
	UseSignTransactionBlockError,
	UseSignTransactionBlockArgs
> {
	const { currentWallet } = useCurrentWallet();
	const currentAccount = useCurrentAccount();
	const client = useSuiClient();

	return useMutation({
		mutationKey: walletMutationKeys.signTransactionBlock(mutationKey),
		mutationFn: async ({ transactionBlock, ...signTransactionBlockArgs }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signTransactionBlockArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign the transaction block with.',
				);
			}

			const walletFeature = currentWallet.features['sui:signTransactionBlock:v2'];
			if (!walletFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `SignTransactionBlock` feature.",
				);
			}

			return await walletFeature.signTransactionBlock({
				...signTransactionBlockArgs,
				transactionBlock: await transactionBlock.toJSON({
					supportedIntents: [],
					client,
				}),
				account: signerAccount,
				chain: signTransactionBlockArgs.chain ?? signerAccount.chains[0],
			});
		},
		...mutationOptions,
	});
}
