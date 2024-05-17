// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TransactionBlock } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import type {
	SuiSignAndExecuteTransactionBlockV2Input,
	SuiSignAndExecuteTransactionBlockV2Output,
} from '@mysten/wallet-standard';
import { signAndExecuteTransactionBlock, signTransactionBlock } from '@mysten/wallet-standard';
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
	Omit<SuiSignAndExecuteTransactionBlockV2Input, 'transactionBlock'>,
	'account' | 'chain'
> & {
	transactionBlock: TransactionBlock | string;
};

type UseSignAndExecuteTransactionBlockResult = SuiSignAndExecuteTransactionBlockV2Output;

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
}: UseSignAndExecuteTransactionBlockMutationOptions = {}): UseMutationResult<
	UseSignAndExecuteTransactionBlockResult,
	UseSignAndExecuteTransactionBlockError,
	UseSignAndExecuteTransactionBlockArgs
> {
	const { currentWallet, supportedIntents } = useCurrentWallet();
	const currentAccount = useCurrentAccount();
	const client = useSuiClient();

	return useMutation({
		mutationKey: walletMutationKeys.signAndExecuteTransactionBlock(mutationKey),
		mutationFn: async ({ transactionBlock, ...signTransactionBlockArgs }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			const signerAccount = signTransactionBlockArgs.account ?? currentAccount;
			if (!signerAccount) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to sign and execute the transaction block with.',
				);
			}

			const reportEffects =
				currentWallet.features['sui:reportTransactionBlockEffects']
					?.reportTransactionBlockEffects ?? (() => {});

			if (
				!currentWallet.features['sui:signTransactionBlock'] &&
				!currentWallet.features['sui:signTransactionBlock:v2']
			) {
				if (
					!currentWallet.features['sui:signAndExecuteTransactionBlock'] &&
					!currentWallet.features['sui:signAndExecuteTransactionBlock:v2']
				) {
					throw new WalletFeatureNotSupportedError(
						"This wallet doesn't support the `signAndExecuteTransactionBlock` feature.",
					);
				}
				return signAndExecuteTransactionBlock(currentWallet, {
					...signTransactionBlockArgs,
					transactionBlock: {
						async toJSON() {
							return typeof transactionBlock === 'string'
								? transactionBlock
								: await transactionBlock.toJSON({
										supportedIntents,
										client,
								  });
						},
					},
					account: signerAccount,
					chain: signTransactionBlockArgs.chain ?? signerAccount.chains[0],
				});
			}

			const { signature, bytes } = await signTransactionBlock(currentWallet, {
				...signTransactionBlockArgs,
				transactionBlock: {
					async toJSON() {
						return typeof transactionBlock === 'string'
							? transactionBlock
							: await transactionBlock.toJSON({
									supportedIntents,
									client,
							  });
					},
				},
				account: signerAccount,
				chain: signTransactionBlockArgs.chain ?? signerAccount.chains[0],
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
