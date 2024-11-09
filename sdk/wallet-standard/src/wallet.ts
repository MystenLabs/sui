// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import { Transaction } from '@mysten/sui/transactions';
import { fromBase64, toBase64 } from '@mysten/sui/utils';
import type { WalletWithFeatures } from '@wallet-standard/core';

import type {
	SuiSignAndExecuteTransactionInput,
	SuiSignTransactionInput,
	SuiWalletFeatures,
} from './features/index.js';

declare module '@wallet-standard/core' {
	export interface Wallet {
		/**
		 * Unique identifier of the Wallet.
		 *
		 * If not provided, the wallet name will be used as the identifier.
		 */
		readonly id?: string;
	}

	export interface StandardConnectOutput {
		supportedIntents?: string[];
	}
}

export type { Wallet } from '@wallet-standard/core';

export async function signAndExecuteTransaction(
	wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>,
	input: SuiSignAndExecuteTransactionInput,
) {
	if (wallet.features['sui:signAndExecuteTransaction']) {
		return wallet.features['sui:signAndExecuteTransaction'].signAndExecuteTransaction(input);
	}

	if (!wallet.features['sui:signAndExecuteTransactionBlock']) {
		throw new Error(
			`Provided wallet (${wallet.name}) does not support the signAndExecuteTransaction feature.`,
		);
	}

	const { signAndExecuteTransactionBlock } = wallet.features['sui:signAndExecuteTransactionBlock'];

	const transactionBlock = Transaction.from(await input.transaction.toJSON());
	const { digest, rawEffects, rawTransaction } = await signAndExecuteTransactionBlock({
		account: input.account,
		chain: input.chain,
		transactionBlock,
		options: {
			showRawEffects: true,
			showRawInput: true,
		},
	});

	const [
		{
			txSignatures: [signature],
			intentMessage: { value: bcsTransaction },
		},
	] = bcs.SenderSignedData.parse(fromBase64(rawTransaction!));

	const bytes = bcs.TransactionData.serialize(bcsTransaction).toBase64();

	return {
		digest,
		signature,
		bytes,
		effects: toBase64(new Uint8Array(rawEffects!)),
	};
}

export async function signTransaction(
	wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>,
	input: SuiSignTransactionInput,
) {
	if (wallet.features['sui:signTransaction']) {
		return wallet.features['sui:signTransaction'].signTransaction(input);
	}

	if (!wallet.features['sui:signTransactionBlock']) {
		throw new Error(
			`Provided wallet (${wallet.name}) does not support the signTransaction feature.`,
		);
	}

	const { signTransactionBlock } = wallet.features['sui:signTransactionBlock'];

	const transaction = Transaction.from(await input.transaction.toJSON());
	const { transactionBlockBytes, signature } = await signTransactionBlock({
		transactionBlock: transaction,
		account: input.account,
		chain: input.chain,
	});

	return { bytes: transactionBlockBytes, signature };
}
