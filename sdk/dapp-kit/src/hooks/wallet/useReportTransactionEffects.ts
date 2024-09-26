// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiReportTransactionEffectsInput } from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import { getReportTransactionEffects } from '../../core/wallet/getReportTransactionEffects.js';
import type {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useWalletStore } from './useWalletStore.js';

type UseReportTransactionEffectsArgs = Omit<
	PartialBy<SuiReportTransactionEffectsInput, 'account' | 'chain'>,
	'effects'
> & {
	effects: string | number[];
};

type UseReportTransactionEffectsError =
	| WalletFeatureNotSupportedError
	| WalletNoAccountSelectedError
	| WalletNotConnectedError
	| Error;

type UseReportTransactionEffectsMutationOptions = Omit<
	UseMutationOptions<
		void,
		UseReportTransactionEffectsError,
		UseReportTransactionEffectsArgs,
		unknown
	>,
	'mutationFn'
>;

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useReportTransactionEffects({
	mutationKey,
	...mutationOptions
}: UseReportTransactionEffectsMutationOptions = {}): UseMutationResult<
	void,
	UseReportTransactionEffectsError,
	UseReportTransactionEffectsArgs
> {
	const reportTransactionEffects = useWalletStore(getReportTransactionEffects);

	return useMutation({
		mutationKey: walletMutationKeys.reportTransactionEffects(mutationKey),
		mutationFn: async (args) => {
			return reportTransactionEffects(args);
		},
		...mutationOptions,
	});
}
