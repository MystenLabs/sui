// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../../bcs/index.js';
import type { ExecuteTransactionBlockParams, SuiClient } from '../../client/index.js';
import type { Signer } from '../../cryptography/keypair.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { ObjectCache } from '../ObjectCache.js';
import type { TransactionBlock } from '../TransactionBlock.js';
import { isTransactionBlock } from '../TransactionBlock.js';

export class CachingTransactionBlockExecutor {
	#client: SuiClient;
	cache: ObjectCache;

	constructor({
		client,
		...options
	}: ObjectCacheOptions & {
		client: SuiClient;
	}) {
		this.#client = client;
		this.cache = new ObjectCache(options);
	}

	/**
	 * Clears all Owned objects
	 * Immutable objects, Shared objects, and Move function definitions will be preserved
	 */
	async reset() {
		await this.cache.clearOwnedObjects();
		await this.cache.clearCustom();
	}

	async buildTransactionBlock({ transactionBlock }: { transactionBlock: TransactionBlock }) {
		transactionBlock.addSerializationPlugin(this.cache.asPlugin());
		return transactionBlock.build({
			client: this.#client,
		});
	}

	async executeTransactionBlock({
		transactionBlock,
		options,
		...input
	}: {
		transactionBlock: TransactionBlock | Uint8Array;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock'>) {
		const results = await this.#client.executeTransactionBlock({
			...input,
			transactionBlock: isTransactionBlock(transactionBlock)
				? await this.buildTransactionBlock({ transactionBlock })
				: transactionBlock,
			options: {
				...options,
				showRawEffects: true,
			},
		});

		if (results.rawEffects) {
			const effects = bcs.TransactionEffects.parse(Uint8Array.from(results.rawEffects));
			await this.applyEffects(effects);
		}

		return results;
	}

	async signAndExecuteTransactionBlock({
		options,
		transactionBlock,
		...input
	}: {
		transactionBlock: TransactionBlock;

		signer: Signer;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock' | 'signature'>) {
		transactionBlock.setSenderIfNotSet(input.signer.toSuiAddress());
		transactionBlock.addBuildPlugin(this.cache.asPlugin());
		const bytes = await this.buildTransactionBlock({ transactionBlock });
		const { signature } = await input.signer.signTransactionBlock(bytes);
		const results = await this.executeTransactionBlock({
			transactionBlock: bytes,
			signature,
			options,
		});

		return results;
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		await this.cache.applyEffects(effects);
	}
}
