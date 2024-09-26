// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/sui/utils';
import type { SuiReportTransactionEffectsInput } from '@mysten/wallet-standard';

import {
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import type { StoreState } from '../../walletStore.js';
import { getCurrentAccount } from './getCurrentAccount.js';
import { getCurrentWallet } from './getCurrentWallet.js';

type UseReportTransactionEffectsArgs = Omit<
	PartialBy<SuiReportTransactionEffectsInput, 'account' | 'chain'>,
	'effects'
> & {
	effects: string | number[];
};

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function getReportTransactionEffects(state: StoreState) {
	const { currentWallet } = getCurrentWallet(state);
	const currentAccount = getCurrentAccount(state);

	return async ({
		effects,
		chain = currentWallet?.chains[0],
		account = currentAccount ?? undefined,
	}: UseReportTransactionEffectsArgs) => {
		if (!currentWallet) {
			throw new WalletNotConnectedError('No wallet is connected.');
		}

		if (!account) {
			throw new WalletNoAccountSelectedError(
				'No wallet account is selected to report transaction effects for',
			);
		}

		const reportTransactionEffectsFeature = currentWallet.features['sui:reportTransactionEffects'];

		if (reportTransactionEffectsFeature) {
			return await reportTransactionEffectsFeature.reportTransactionEffects({
				effects: Array.isArray(effects) ? toB64(new Uint8Array(effects)) : effects,
				account,
				chain: chain ?? currentWallet?.chains[0],
			});
		}
	};
}
