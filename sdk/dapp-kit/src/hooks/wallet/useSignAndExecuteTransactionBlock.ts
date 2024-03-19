// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	SuiSignAndExecuteTransactionBlockInput,
	SuiSignAndExecuteTransactionBlockOutput,
} from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useSuiClient } from '../useSuiClient.js';
import { useCurrentAccount } from './useCurrentAccount.js';
import { useCurrentWallet } from './useCurrentWallet.js';

type UseSignAndExecuteTransactionBlockArgs = PartialBy<
	SuiSignAndExecuteTransactionBlockInput,
	'account' | 'chain'
>;

type UseSignAndExecuteTransactionBlockResult = SuiSignAndExecuteTransactionBlockOutput;

type UseSignAndExecuteTransactionBlockError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignAndExecuteTransactionBlockMutationOptions = Omit<
	UseMutationOptions<
		UseSignAndExecuteTransactionBlockResult,
		UseSignAndExecuteTransactionBlockError,
		UseSignAndExecuteTransactionBlockArgs,
		unknown
	>,
	'mutationFn'
> & {
	executeFromWallet?: boolean;
};

/**
 * Mutation hook for prompting the user to sign and execute a transaction block.
 */
export function useSignAndExecuteTransactionBlock({
	mutationKey,
	executeFromWallet,
	...mutationOptions
}: UseSignAndExecuteTransactionBlockMutationOptions = {}): UseMutationResult<
	UseSignAndExecuteTransactionBlockResult,
	UseSignAndExecuteTransactionBlockError,
	UseSignAndExecuteTransactionBlockArgs
> {
	const { currentWallet } = useCurrentWallet();
	const currentAccount = useCurrentAccount();
	const client = useSuiClient();

	return useMutation({
		mutationKey: walletMutationKeys.signAndExecuteTransactionBlock(mutationKey),
		mutationFn: async ({ requestType, options, ...signTransactionBlockArgs }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signTransactionBlockArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign and execute the transaction block with.',
				);
			}

			if (executeFromWallet) {
				const walletFeature = currentWallet.features['sui:signAndExecuteTransactionBlock'];
				if (!walletFeature) {
					throw new WalletFeatureNotSupportedError(
						"This wallet doesn't support the `signAndExecuteTransactionBlock` feature.",
					);
				}

				return walletFeature.signAndExecuteTransactionBlock({
					...signTransactionBlockArgs,
					account: signerAccount,
					chain: signTransactionBlockArgs.chain ?? signerAccount.chains[0],
					requestType,
					options,
				});
			}

			const walletFeature = currentWallet.features['sui:signTransactionBlock'];
			if (!walletFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signTransactionBlock` feature.",
				);
			}

			const { signature, transactionBlockBytes } = await walletFeature.signTransactionBlock({
				...signTransactionBlockArgs,
				account: signerAccount,
				chain: signTransactionBlockArgs.chain ?? signerAccount.chains[0],
			});

			return client.executeTransactionBlock({
				transactionBlock: transactionBlockBytes,
				signature,
				requestType,
				options,
			});
		},
		...mutationOptions,
	});
}
