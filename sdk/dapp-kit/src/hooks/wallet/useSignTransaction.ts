// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Transaction } from '@mysten/sui/transactions';
import type { SignedTransaction, SuiSignTransactionInput } from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getSignTransaction } from '../../core/wallet/getSignTransaction.js';
import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useSuiClient } from '../useSuiClient.js';
import { useWalletStore } from './useWalletStore.js';

type UseSignTransactionArgs = PartialBy<
	Omit<SuiSignTransactionInput, 'transaction'>,
	'account' | 'chain'
> & {
	transaction: Transaction | string;
};

interface UseSignTransactionResult extends SignedTransaction {
	reportTransactionEffects: (effects: string) => void;
}

type UseSignTransactionError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseSignTransactionMutationOptions = Omit<
	UseMutationOptions<
		UseSignTransactionResult,
		UseSignTransactionError,
		UseSignTransactionArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a transaction.
 */
export function useSignTransaction({
	mutationKey,
	...mutationOptions
}: UseSignTransactionMutationOptions = {}): UseMutationResult<
	UseSignTransactionResult,
	UseSignTransactionError,
	UseSignTransactionArgs
> {
	const client = useSuiClient();

	const signTransaction = useWalletStore((state) => getSignTransaction(client, state));

	return useMutation({
		mutationKey: walletMutationKeys.signTransaction(mutationKey),
		mutationFn: async (args) => {
			return signTransaction(args);
		},
		...mutationOptions,
	});
}
