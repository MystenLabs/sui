// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import { TransactionBlock } from '@mysten/sui/transactions';
import { fromB64, toB64 } from '@mysten/sui/utils';
import type { WalletWithFeatures } from '@wallet-standard/core';

import type {
	SuiSignAndExecuteTransactionBlockV2Input,
	SuiSignTransactionBlockV2Input,
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

export async function signAndExecuteTransactionBlock(
	wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>,
	input: SuiSignAndExecuteTransactionBlockV2Input,
) {
	if (wallet.features['sui:signAndExecuteTransactionBlock:v2']) {
		return wallet.features['sui:signAndExecuteTransactionBlock:v2'].signAndExecuteTransactionBlock(
			input,
		);
	}

	if (!wallet.features['sui:signAndExecuteTransactionBlock']) {
		throw new Error(
			`Provided wallet (${wallet.name}) does not support the signAndExecuteTransactionBlock feature.`,
		);
	}

	const { signAndExecuteTransactionBlock } = wallet.features['sui:signAndExecuteTransactionBlock'];

	const transactionBlock = TransactionBlock.from(await input.transactionBlock.toJSON());
	const { digest, rawEffects, rawTransaction } = await signAndExecuteTransactionBlock({
		...input,
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
	] = bcs.SenderSignedData.parse(fromB64(rawTransaction!));

	const bytes = bcs.TransactionData.serialize(bcsTransaction).toBase64();

	return {
		digest,
		signature,
		bytes,
		effects: toB64(new Uint8Array(rawEffects!)),
	};
}

export async function signTransactionBlock(
	wallet: WalletWithFeatures<Partial<SuiWalletFeatures>>,
	input: SuiSignTransactionBlockV2Input,
) {
	if (wallet.features['sui:signTransactionBlock:v2']) {
		return wallet.features['sui:signTransactionBlock:v2'].signTransactionBlock(input);
	}

	if (!wallet.features['sui:signTransactionBlock']) {
		throw new Error(
			`Provided wallet (${wallet.name}) does not support the signTransactionBlock feature.`,
		);
	}

	const { signTransactionBlock } = wallet.features['sui:signTransactionBlock'];

	const transactionBlock = TransactionBlock.from(await input.transactionBlock.toJSON());
	const { transactionBlockBytes, signature } = await signTransactionBlock({
		...input,
		transactionBlock,
	});

	return { bytes: transactionBlockBytes, signature };
}
