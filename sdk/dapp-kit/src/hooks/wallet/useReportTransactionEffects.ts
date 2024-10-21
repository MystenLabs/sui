// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toBase64 } from '@mysten/sui/utils';
import type { SuiReportTransactionEffectsInput } from '@mysten/wallet-standard';
import type { UseMutationOptions, UseMutationResult } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';

import { walletMutationKeys } from '../../constants/walletMutationKeys.js';
import type { WalletFeatureNotSupportedError } from '../../errors/walletErrors.js';
import {
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import { useCurrentAccount } from './useCurrentAccount.js';
import { useCurrentWallet } from './useCurrentWallet.js';

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
	const { currentWallet } = useCurrentWallet();
	const currentAccount = useCurrentAccount();

	return useMutation({
		mutationKey: walletMutationKeys.reportTransactionEffects(mutationKey),
		mutationFn: async ({ effects, chain = currentWallet?.chains[0], account = currentAccount }) => {
			if (!currentWallet) {
				throw new WalletNotConnectedError('No wallet is connected.');
			}

			if (!account) {
				throw new WalletNoAccountSelectedError(
					'No wallet account is selected to report transaction effects for',
				);
			}

			const reportTransactionEffectsFeature =
				currentWallet.features['sui:reportTransactionEffects'];

			if (reportTransactionEffectsFeature) {
				return await reportTransactionEffectsFeature.reportTransactionEffects({
					effects: Array.isArray(effects) ? toBase64(new Uint8Array(effects)) : effects,
					account,
					chain: chain ?? currentWallet?.chains[0],
				});
			}
		},
		...mutationOptions,
	});
}
