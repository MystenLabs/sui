// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction } from '@mysten/sui/transactions';
import type {
	SuiSignAndExecuteTransactionInput,
	SuiSignAndExecuteTransactionOutput,
} from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useWalletStore } from './useWalletStore.js';

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

type ExecuteTransactionResult =
	| {
			digest: string;
			rawEffects?: number[];
	  }
	| {
			effects?: {
				bcs?: string;
			};
	  };

type UseSignAndExecuteTransactionMutationOptions<Result extends ExecuteTransactionResult> = Omit<
	UseMutationOptions<
		Result,
		UseSignAndExecuteTransactionError,
		UseSignAndExecuteTransactionArgs,
		unknown
	>,
	'mutationFn'
> & {
	execute?: ({ bytes, signature }: { bytes: string; signature: string }) => Promise<Result>;
};

/**
 * Mutation hook for prompting the user to sign and execute a transaction.
 */
export function useSignAndExecuteTransaction<
	Result extends ExecuteTransactionResult = UseSignAndExecuteTransactionResult,
>({
	mutationKey,
	execute,
	...mutationOptions
}: UseSignAndExecuteTransactionMutationOptions<Result> = {}): UseMutationResult<
	Result,
	UseSignAndExecuteTransactionError,
	UseSignAndExecuteTransactionArgs
> {
	const store = useWalletStore();

	return useMutation({
		mutationKey: walletMutationKeys.signAndExecuteTransaction(mutationKey),
		mutationFn: async (args) => {
			return store.signAndExecuteTransaction(args);
		},
		...mutationOptions,
	});
}
