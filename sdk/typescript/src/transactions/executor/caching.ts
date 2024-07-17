// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../../bcs/index.js';
import type { ExecuteTransactionBlockParams, SuiClient } from '../../client/index.js';
import type { Signer } from '../../cryptography/keypair.js';
import type { BuildTransactionOptions } from '../json-rpc-resolver.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { ObjectCache } from '../ObjectCache.js';
import type { Transaction } from '../Transaction.js';
import { isTransaction } from '../Transaction.js';

export class CachingTransactionExecutor {
	#client: SuiClient;
	#lastDigest: string | null = null;
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
		await Promise.all([
			this.cache.clearOwnedObjects(),
			this.cache.clearCustom(),
			this.waitForLastTransaction(),
		]);
	}

	async buildTransaction({
		transaction,
		...options
	}: { transaction: Transaction } & BuildTransactionOptions) {
		transaction.addBuildPlugin(this.cache.asPlugin());
		return transaction.build({
			client: this.#client,
			...options,
		});
	}

	async executeTransaction({
		transaction,
		options,
		...input
	}: {
		transaction: Transaction | Uint8Array;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock'>) {
		const bytes = isTransaction(transaction)
			? await this.buildTransaction({ transaction })
			: transaction;

		const results = await this.#client.executeTransactionBlock({
			...input,
			transactionBlock: bytes,
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

	async signAndExecuteTransaction({
		options,
		transaction,
		...input
	}: {
		transaction: Transaction;

		signer: Signer;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock' | 'signature'>) {
		transaction.setSenderIfNotSet(input.signer.toSuiAddress());
		const bytes = await this.buildTransaction({ transaction });
		const { signature } = await input.signer.signTransaction(bytes);
		const results = await this.executeTransaction({
			transaction: bytes,
			signature,
			options,
		});

		return results;
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		this.#lastDigest = effects.V2?.transactionDigest ?? null;
		await this.cache.applyEffects(effects);
	}

	async waitForLastTransaction() {
		if (this.#lastDigest) {
			await this.#client.waitForTransaction({ digest: this.#lastDigest });
			this.#lastDigest = null;
		}
	}
}
