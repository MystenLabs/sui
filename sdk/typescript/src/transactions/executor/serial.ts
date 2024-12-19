// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toBase64 } from '@mysten/bcs';

import type { bcs } from '../../bcs/index.js';
import type { SuiClient, SuiTransactionBlockResponseOptions } from '../../client/index.js';
import type { Signer } from '../../cryptography/keypair.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { isTransaction, Transaction } from '../Transaction.js';
import { CachingTransactionExecutor } from './caching.js';
import { SerialQueue } from './queue.js';

export class SerialTransactionExecutor {
	#queue = new SerialQueue();
	#signer: Signer;
	#cache: CachingTransactionExecutor;
	#defaultGasBudget: bigint;

	constructor({
		signer,
		defaultGasBudget = 50_000_000n,
		...options
	}: Omit<ObjectCacheOptions, 'address'> & {
		client: SuiClient;
		signer: Signer;
		/** The gasBudget to use if the transaction has not defined it's own gasBudget, defaults to `50_000_000n` */
		defaultGasBudget?: bigint;
	}) {
		this.#signer = signer;
		this.#defaultGasBudget = defaultGasBudget;
		this.#cache = new CachingTransactionExecutor({
			client: options.client,
			cache: options.cache,
			onEffects: (effects) => this.#cacheGasCoin(effects),
		});
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		return this.#cache.applyEffects(effects);
	}

	#cacheGasCoin = async (effects: typeof bcs.TransactionEffects.$inferType) => {
		if (!effects.V2) {
			return;
		}

		const gasCoin = getGasCoinFromEffects(effects).ref;
		if (gasCoin) {
			this.#cache.cache.setCustom('gasCoin', gasCoin);
		} else {
			this.#cache.cache.deleteCustom('gasCoin');
		}
	};

	async buildTransaction(transaction: Transaction) {
		return this.#queue.runTask(() => this.#buildTransaction(transaction));
	}

	#buildTransaction = async (transaction: Transaction) => {
		const gasCoin = await this.#cache.cache.getCustom<{
			objectId: string;
			version: string;
			digest: string;
		}>('gasCoin');

		const copy = Transaction.from(transaction);
		if (gasCoin) {
			copy.setGasPayment([gasCoin]);
		}

		copy.setGasBudgetIfNotSet(this.#defaultGasBudget);
		copy.setSenderIfNotSet(this.#signer.toSuiAddress());

		return this.#cache.buildTransaction({ transaction: copy });
	};

	resetCache() {
		return this.#cache.reset();
	}

	waitForLastTransaction() {
		return this.#cache.waitForLastTransaction();
	}

	executeTransaction(
		transaction: Transaction | Uint8Array,
		options?: SuiTransactionBlockResponseOptions,
	) {
		return this.#queue.runTask(async () => {
			const bytes = isTransaction(transaction)
				? await this.#buildTransaction(transaction)
				: transaction;

			const { signature } = await this.#signer.signTransaction(bytes);
			const results = await this.#cache
				.executeTransaction({
					signature,
					transaction: bytes,
					options,
				})
				.catch(async (error) => {
					await this.resetCache();
					throw error;
				});

			const effectsBytes = Uint8Array.from(results.rawEffects!);
			return {
				digest: results.digest,
				effects: toBase64(effectsBytes),
				data: results,
			};
		});
	}
}

export function getGasCoinFromEffects(effects: typeof bcs.TransactionEffects.$inferType) {
	if (!effects.V2) {
		throw new Error('Unexpected effects version');
	}

	const gasObjectChange = effects.V2.changedObjects[effects.V2.gasObjectIndex!];

	if (!gasObjectChange) {
		throw new Error('Gas object not found in effects');
	}

	const [objectId, { outputState }] = gasObjectChange;

	if (!outputState.ObjectWrite) {
		throw new Error('Unexpected gas object state');
	}

	const [digest, owner] = outputState.ObjectWrite;

	return {
		ref: {
			objectId,
			digest,
			version: effects.V2.lamportVersion,
		},
		owner: owner.AddressOwner || owner.ObjectOwner!,
	};
}
