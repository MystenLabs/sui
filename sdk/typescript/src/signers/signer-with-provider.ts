// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { TransactionBlock } from '../builder/TransactionBlock.js';
import { isTransactionBlock } from '../builder/TransactionBlock.js';
import { TransactionBlockDataBuilder } from '../builder/TransactionBlockData.js';
import type {
	DevInspectResults,
	DryRunTransactionBlockResponse,
	ExecuteTransactionRequestType,
	SuiClient,
	SuiTransactionBlockResponse,
	SuiTransactionBlockResponseOptions,
} from '../client/index.js';
import { IntentScope, messageWithIntent } from '../cryptography/intent.js';
import type { SerializedSignature } from '../cryptography/signature.js';
import type { Signer } from './signer.js';
import type { SignedMessage, SignedTransaction } from './types.js';

///////////////////////////////
// Exported Abstracts
export abstract class SignerWithProvider implements Signer {
	readonly client: SuiClient;

	///////////////////
	// Sub-classes MUST implement these

	// Returns the checksum address
	abstract getAddress(): Promise<string>;

	/**
	 * Returns the signature for the data and the public key of the signer
	 */
	abstract signData(data: Uint8Array): Promise<SerializedSignature>;

	// Returns a new instance of the Signer, connected to provider.
	// This MAY throw if changing providers is not supported.
	abstract connect(client: SuiClient): SignerWithProvider;

	///////////////////
	// Sub-classes MAY override these

	constructor(client: SuiClient) {
		this.client = client as SuiClient;
	}

	/**
	 * Sign a message using the keypair, with the `PersonalMessage` intent.
	 */
	async signMessage(input: { message: Uint8Array }): Promise<SignedMessage> {
		const signature = await this.signData(
			messageWithIntent(
				IntentScope.PersonalMessage,
				bcs.vector(bcs.u8()).serialize(input.message).toBytes(),
			),
		);

		return {
			messageBytes: toB64(input.message),
			signature,
		};
	}

	protected async prepareTransactionBlock(transactionBlock: Uint8Array | TransactionBlock) {
		if (isTransactionBlock(transactionBlock)) {
			// If the sender has not yet been set on the transaction, then set it.
			// NOTE: This allows for signing transactions with mis-matched senders, which is important for sponsored transactions.
			transactionBlock.setSenderIfNotSet(await this.getAddress());
			return await transactionBlock.build({
				client: this.client,
			});
		}
		if (transactionBlock instanceof Uint8Array) {
			return transactionBlock;
		}
		throw new Error('Unknown transaction format');
	}

	/**
	 * Sign a transaction.
	 */
	async signTransactionBlock(input: {
		transactionBlock: Uint8Array | TransactionBlock;
	}): Promise<SignedTransaction> {
		const transactionBlockBytes = await this.prepareTransactionBlock(input.transactionBlock);

		const intentMessage = messageWithIntent(IntentScope.TransactionData, transactionBlockBytes);
		const signature = await this.signData(intentMessage);

		return {
			transactionBlockBytes: toB64(transactionBlockBytes),
			signature,
		};
	}

	/**
	 * Sign a transaction block and submit to the Fullnode for execution.
	 *
	 * @param options specify which fields to return (e.g., transaction, effects, events, etc).
	 * By default, only the transaction digest will be returned.
	 * @param requestType WaitForEffectsCert or WaitForLocalExecution, see details in `ExecuteTransactionRequestType`.
	 * Defaults to `WaitForLocalExecution` if options.show_effects or options.show_events is true
	 */
	async signAndExecuteTransactionBlock(input: {
		transactionBlock: Uint8Array | TransactionBlock;
		/** specify which fields to return (e.g., transaction, effects, events, etc). By default, only the transaction digest will be returned. */
		options?: SuiTransactionBlockResponseOptions;
		/** `WaitForEffectsCert` or `WaitForLocalExecution`, see details in `ExecuteTransactionRequestType`.
		 * Defaults to `WaitForLocalExecution` if options.show_effects or options.show_events is true
		 */
		requestType?: ExecuteTransactionRequestType;
	}): Promise<SuiTransactionBlockResponse> {
		const { transactionBlockBytes, signature } = await this.signTransactionBlock({
			transactionBlock: input.transactionBlock,
		});

		return await this.client.executeTransactionBlock({
			transactionBlock: transactionBlockBytes,
			signature,
			options: input.options,
			requestType: input.requestType,
		});
	}

	/**
	 * Derive transaction digest from
	 * @param tx BCS serialized transaction data or a `Transaction` object
	 * @returns transaction digest
	 */
	async getTransactionBlockDigest(tx: Uint8Array | TransactionBlock): Promise<string> {
		if (isTransactionBlock(tx)) {
			tx.setSenderIfNotSet(await this.getAddress());
			return tx.getDigest({ client: this.client });
		} else if (tx instanceof Uint8Array) {
			return TransactionBlockDataBuilder.getDigestFromBytes(tx);
		} else {
			throw new Error('Unknown transaction format.');
		}
	}

	/**
	 * Runs the transaction in dev-inpsect mode. Which allows for nearly any
	 * transaction (or Move call) with any arguments. Detailed results are
	 * provided, including both the transaction effects and any return values.
	 */
	async devInspectTransactionBlock(
		input: Omit<Parameters<SuiClient['devInspectTransactionBlock']>[0], 'sender'>,
	): Promise<DevInspectResults> {
		const address = await this.getAddress();
		return this.client.devInspectTransactionBlock({
			sender: address,
			...input,
		});
	}

	/**
	 * Dry run a transaction and return the result.
	 */
	async dryRunTransactionBlock(input: {
		transactionBlock: TransactionBlock | string | Uint8Array;
	}): Promise<DryRunTransactionBlockResponse> {
		let dryRunTxBytes: Uint8Array;
		if (isTransactionBlock(input.transactionBlock)) {
			input.transactionBlock.setSenderIfNotSet(await this.getAddress());
			dryRunTxBytes = await input.transactionBlock.build({
				client: this.client,
			});
		} else if (typeof input.transactionBlock === 'string') {
			dryRunTxBytes = fromB64(input.transactionBlock);
		} else if (input.transactionBlock instanceof Uint8Array) {
			dryRunTxBytes = input.transactionBlock;
		} else {
			throw new Error('Unknown transaction format');
		}

		return this.client.dryRunTransactionBlock({
			transactionBlock: dryRunTxBytes,
		});
	}
}
