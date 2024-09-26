// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import type { Transaction } from '@mysten/sui/transactions';
import { signTransaction } from '@mysten/wallet-standard';
import type { SuiSignTransactionInput } from '@mysten/wallet-standard';

import {
	WalletFeatureNotSupportedError,
	WalletNoAccountSelectedError,
	WalletNotConnectedError,
} from '../../errors/walletErrors.js';
import type { PartialBy } from '../../types/utilityTypes.js';
import type { StoreState } from '../../walletStore.js';
import { getCurrentAccount } from './getCurrentAccount.js';
import { getCurrentWallet } from './getCurrentWallet.js';
import { getReportTransactionEffects } from './getReportTransactionEffects.js';

type UseSignTransactionArgs = PartialBy<
	Omit<SuiSignTransactionInput, 'transaction'>,
	'account' | 'chain'
> & {
	transaction: Transaction | string;
};

/**
 * Mutation hook for prompting the user to sign a transaction.
 */
export function getSignTransaction(client: SuiClient, state: StoreState) {
	const { currentWallet } = getCurrentWallet(state);
	const currentAccount = getCurrentAccount(state);

	const reportTransactionEffects = getReportTransactionEffects(state);

	return async ({ transaction, ...signTransactionArgs }: UseSignTransactionArgs) => {
		if (!currentWallet) {
			throw new WalletNotConnectedError('No wallet is connected.');
		}

		const signerAccount = signTransactionArgs.account ?? currentAccount;
		if (!signerAccount) {
			throw new WalletNoAccountSelectedError(
				'No wallet account is selected to sign the transaction with.',
			);
		}

		if (
			!currentWallet.features['sui:signTransaction'] &&
			!currentWallet.features['sui:signTransactionBlock']
		) {
			throw new WalletFeatureNotSupportedError(
				"This wallet doesn't support the `signTransaction` feature.",
			);
		}

		const { bytes, signature } = await signTransaction(currentWallet, {
			...signTransactionArgs,
			transaction: {
				toJSON: async () => {
					return typeof transaction === 'string'
						? transaction
						: await transaction.toJSON({
								supportedIntents: [],
								client,
							});
				},
			},
			account: signerAccount,
			chain: signTransactionArgs.chain ?? signerAccount.chains[0],
		});

		return {
			bytes,
			signature,
			reportTransactionEffects: (effects: string) => {
				reportTransactionEffects({
					effects,
					account: signerAccount,
					chain: signTransactionArgs.chain ?? signerAccount.chains[0],
				});
			},
		};
	};
}
