// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type ExecuteTransactionRequestType,
	type SerializedSignature,
	type SignedMessage,
	type SignedTransaction,
	SignerWithProvider,
	type SuiTransactionBlockResponse,
	type SuiTransactionBlockResponseOptions,
	type TransactionBlock,
} from '@mysten/sui.js';

export abstract class WalletSigner extends SignerWithProvider {
	abstract signData(data: Uint8Array, clientIdentifier?: string): Promise<SerializedSignature>;

	async signMessage(
		input: { message: Uint8Array },
		clientIdentifier?: string,
	): Promise<SignedMessage> {
		return super.signMessage(input);
	}
	async signTransactionBlock(
		input: {
			transactionBlock: Uint8Array | TransactionBlock;
		},
		clientIdentifier?: string,
	): Promise<SignedTransaction> {
		return super.signTransactionBlock(input);
	}
	async signAndExecuteTransactionBlock(
		input: {
			transactionBlock: Uint8Array | TransactionBlock;
			options?: SuiTransactionBlockResponseOptions;
			requestType?: ExecuteTransactionRequestType;
		},
		clientIdentifier?: string,
	): Promise<SuiTransactionBlockResponse> {
		return super.signAndExecuteTransactionBlock(input);
	}
}
