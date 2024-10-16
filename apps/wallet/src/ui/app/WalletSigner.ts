// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui/bcs';
import {
	type DryRunTransactionBlockResponse,
	type ExecuteTransactionRequestType,
	type SuiClient,
	type SuiTransactionBlockResponse,
	type SuiTransactionBlockResponseOptions,
} from '@mysten/sui/client';
import { messageWithIntent } from '@mysten/sui/cryptography';
import { isTransaction, type Transaction } from '@mysten/sui/transactions';
import { fromBase64, toBase64 } from '@mysten/sui/utils';

export type SignedTransaction = {
	transactionBlockBytes: string;
	signature: string;
};

export type SignedMessage = {
	messageBytes: string;
	signature: string;
};

export abstract class WalletSigner {
	client: SuiClient;

	constructor(client: SuiClient) {
		this.client = client;
	}

	abstract signData(data: Uint8Array, clientIdentifier?: string): Promise<string>;

	abstract getAddress(): Promise<string>;

	async signMessage(
		input: { message: Uint8Array },
		clientIdentifier?: string,
	): Promise<SignedMessage> {
		const signature = await this.signData(
			messageWithIntent('PersonalMessage', bcs.vector(bcs.u8()).serialize(input.message).toBytes()),
		);

		return {
			messageBytes: toBase64(input.message),
			signature,
		};
	}

	protected async prepareTransactionBlock(transactionBlock: Uint8Array | Transaction | string) {
		if (isTransaction(transactionBlock)) {
			// If the sender has not yet been set on the transaction, then set it.
			// NOTE: This allows for signing transactions with mis-matched senders, which is important for sponsored transactions.
			transactionBlock.setSenderIfNotSet(await this.getAddress());
			return await transactionBlock.build({
				client: this.client,
			});
		}

		if (typeof transactionBlock === 'string') {
			return fromBase64(transactionBlock);
		}

		if (transactionBlock instanceof Uint8Array) {
			return transactionBlock;
		}
		throw new Error('Unknown transaction format');
	}

	async signTransactionBlock(
		input: {
			transactionBlock: Uint8Array | Transaction;
		},
		clientIdentifier?: string,
	): Promise<SignedTransaction> {
		const bytes = await this.prepareTransactionBlock(input.transactionBlock);
		const signature = await this.signData(messageWithIntent('TransactionData', bytes));

		return {
			transactionBlockBytes: toBase64(bytes),
			signature,
		};
	}

	async signAndExecuteTransactionBlock(
		input: {
			transactionBlock: Uint8Array | Transaction;
			options?: SuiTransactionBlockResponseOptions;
			requestType?: ExecuteTransactionRequestType;
		},
		clientIdentifier?: string,
	): Promise<SuiTransactionBlockResponse> {
		const bytes = await this.prepareTransactionBlock(input.transactionBlock);
		const signed = await this.signTransactionBlock({
			transactionBlock: bytes,
		});

		return this.client.executeTransactionBlock({
			transactionBlock: bytes,
			signature: signed.signature,
			options: input.options,
			requestType: input.requestType,
		});
	}

	async dryRunTransactionBlock(input: {
		transactionBlock: Transaction | string | Uint8Array;
	}): Promise<DryRunTransactionBlockResponse> {
		return this.client.dryRunTransactionBlock({
			transactionBlock: await this.prepareTransactionBlock(input.transactionBlock),
		});
	}
}
