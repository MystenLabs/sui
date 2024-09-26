// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import type { Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
import type {
	SuiSignAndExecuteTransactionInput,
	SuiSignAndExecuteTransactionOutput,
} from '@mysten/wallet-standard';
import { signTransaction } from '@mysten/wallet-standard';

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

type UseSignAndExecuteTransactionArgs = PartialBy<
	Omit<SuiSignAndExecuteTransactionInput, 'transaction'>,
	'account' | 'chain'
> & {
	transaction: Transaction | string;
};

type UseSignAndExecuteTransactionResult = SuiSignAndExecuteTransactionOutput;

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

/**
 * Mutation hook for prompting the user to sign and execute a transaction.
 */
export function getSignAndExecuteTransaction<
	Result extends ExecuteTransactionResult = UseSignAndExecuteTransactionResult,
>(
	client: SuiClient,
	state: StoreState,
	execute?: ({ bytes, signature }: { bytes: string; signature: string }) => Promise<Result>,
) {
	const { currentWallet, supportedIntents } = getCurrentWallet(state);
	const currentAccount = getCurrentAccount(state);

	const reportTransactionEffects = getReportTransactionEffects(state);

	const executeTransaction: ({
		bytes,
		signature,
	}: {
		bytes: string;
		signature: string;
	}) => Promise<ExecuteTransactionResult> =
		execute ??
		(async ({ bytes, signature }) => {
			const { digest, rawEffects } = await client.executeTransactionBlock({
				transactionBlock: bytes,
				signature,
				options: {
					showRawEffects: true,
				},
			});

			return {
				digest,
				rawEffects,
				effects: toB64(new Uint8Array(rawEffects!)),
				bytes,
				signature,
			};
		});

	return async ({
		transaction,
		...signTransactionArgs
	}: UseSignAndExecuteTransactionArgs): Promise<Result> => {
		if (!currentWallet) {
			throw new WalletNotConnectedError('No wallet is connected.');
		}

		const signerAccount = signTransactionArgs.account ?? currentAccount;
		if (!signerAccount) {
			throw new WalletNoAccountSelectedError(
				'No wallet account is selected to sign the transaction with.',
			);
		}
		const chain = signTransactionArgs.chain ?? signerAccount?.chains[0];

		if (
			!currentWallet.features['sui:signTransaction'] &&
			!currentWallet.features['sui:signTransactionBlock']
		) {
			throw new WalletFeatureNotSupportedError(
				"This wallet doesn't support the `signTransaction` feature.",
			);
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

		const result = await executeTransaction({ bytes, signature });

		let effects: string;

		if ('effects' in result && result.effects?.bcs) {
			effects = result.effects.bcs;
		} else if ('rawEffects' in result) {
			effects = toB64(new Uint8Array(result.rawEffects!));
		} else {
			throw new Error('Could not parse effects from transaction result.');
		}

		reportTransactionEffects({ effects, account: signerAccount, chain });

		return result as Result;
	};
}
