// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	SuiSignAndExecuteTransactionBlockInput,
	SuiSignAndExecuteTransactionBlockOutput,
} from '@mysten/wallet-standard';
import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
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
>;

/**
 * Mutation hook for prompting the user to sign and execute a transaction block.
 */
export function useSignAndExecuteTransactionBlock({
	mutationKey,
	...mutationOptions
}: UseSignAndExecuteTransactionBlockMutationOptions = {}) {
	const currentWallet = useCurrentWallet();
	const currentAccount = useCurrentAccount();

	return useMutation({
		mutationKey: walletMutationKeys.signAndExecuteTransactionBlock(mutationKey),
		mutationFn: async (signAndExecuteTransactionBlockArgs) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signAndExecuteTransactionBlockArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign and execute the transaction block with.',
				);
			}

			const walletFeature = currentWallet.features['sui:signAndExecuteTransactionBlock'];
			if (!walletFeature) {
				throw new WalletFeatureNotSupportedError(
					"This wallet doesn't support the `signAndExecuteTransactionBlock` feature.",
				);
			}

			return await walletFeature.signAndExecuteTransactionBlock({
				...signAndExecuteTransactionBlockArgs,
				account: signerAccount,
				chain: signAndExecuteTransactionBlockArgs.chain ?? signerAccount.chains[0],
			});
		},
		...mutationOptions,
	});
}
