// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import type {
	SuiSignAndExecuteTransactionInput,
	SuiSignAndExecuteTransactionOutput,
} from '@mysten/wallet-standard';
import { signAndExecuteTransaction, signTransaction } from '@mysten/wallet-standard';
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

type UseSignAndExecuteTransactionArgs = PartialBy<
	Omit<SuiSignAndExecuteTransactionInput, 'transaction'>,
	'account' | 'chain'
> & {
	transaction: Transaction | string;
};

type UseSignAndExecuteTransactionResult = SuiSignAndExecuteTransactionOutput;

type UseSignAndExecuteTransactionError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignAndExecuteTransactionMutationOptions = Omit<
	UseMutationOptions<
		UseSignAndExecuteTransactionResult,
		UseSignAndExecuteTransactionError,
		UseSignAndExecuteTransactionArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign and execute a transaction.
 */
export function useSignAndExecuteTransaction({
	mutationKey,
	...mutationOptions
}: UseSignAndExecuteTransactionMutationOptions = {}): UseMutationResult<
	UseSignAndExecuteTransactionResult,
	UseSignAndExecuteTransactionError,
	UseSignAndExecuteTransactionArgs
> {
	const { currentWallet, supportedIntents } = useCurrentWallet();
	const currentAccount = useCurrentAccount();
	const client = useSuiClient();

	return useMutation({
		mutationKey: walletMutationKeys.signAndExecuteTransaction(mutationKey),
		mutationFn: async ({ transaction, ...signTransactionArgs }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signTransactionArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign and execute the transaction with.',
				);
			}

			const reportEffects =
				currentWallet.features['sui:reportTransactionEffects']?.reportTransactionEffects ??
				(() => {});

			if (
				!currentWallet.features['sui:signTransaction'] &&
				!currentWallet.features['sui:signTransactionBlock']
			) {
				if (
					!currentWallet.features['sui:signAndExecuteTransaction'] &&
					!currentWallet.features['sui:signAndExecuteTransactionBlock']
				) {
					throw new WalletFeatureNotSupportedError(
						"This wallet doesn't support the `signAndExecuteTransaction` feature.",
					);
				}
				return signAndExecuteTransaction(currentWallet, {
					...signTransactionArgs,
					transaction: {
						async toJSON() {
							return typeof transaction === 'string'
								? transaction
								: await transaction.toJSON({
										supportedIntents,
										client,
								  });
						},
					},
					account: signerAccount,
					chain: signTransactionArgs.chain ?? signerAccount.chains[0],
				});
			}

			const { signature, bytes } = await signTransaction(currentWallet, {
				...signTransactionArgs,
				transaction: {
					async toJSON() {
						return typeof transaction === 'string'
							? transaction
							: await transaction.toJSON({
									supportedIntents,
									client,
							  });
					},
				},
				account: signerAccount,
				chain: signTransactionArgs.chain ?? signerAccount.chains[0],
			});

			const { rawEffects, digest } = await client.executeTransactionBlock({
				transactionBlock: bytes,
				signature,
				options: {
					showRawEffects: true,
				},
			});

			const effects = toB64(new Uint8Array(rawEffects!));

			await reportEffects({ effects });

			return {
				digest,
				bytes,
				signature,
				effects,
			};
		},
		...mutationOptions,
	});
}
