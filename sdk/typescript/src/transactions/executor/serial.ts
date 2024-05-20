// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';

import { bcs } from '../../bcs/index.js';
import type { SuiClient } from '../../client/index.js';
import type { Signer } from '../../cryptography/keypair.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { isTransactionBlock, TransactionBlock } from '../TransactionBlock.js';
import { CachingTransactionBlockExecutor } from './caching.js';
import { SerialQueue } from './queue.js';

export class SerialTransactionBlockExecutor {
	#queue = new SerialQueue();
	#signer: Signer;
	#cache: CachingTransactionBlockExecutor;

	constructor({
		signer,
		...options
	}: Omit<ObjectCacheOptions, 'address'> & {
		client: SuiClient;
		signer: Signer;
	}) {
		this.#signer = signer;
		this.#cache = new CachingTransactionBlockExecutor({
			address: this.#signer.toSuiAddress(),
			client: options.client,
			cache: options.cache,
		});
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		return Promise.all([this.#cacheGasCoin(effects), this.#cache.cache.applyEffects(effects)]);
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

	#buildTransactionBlock = async (transactionBlock: TransactionBlock) => {
		const gasCoin = await this.#cache.cache.getCustom<{
			objectId: string;
			version: string;
			digest: string;
		}>('gasCoin');

		const copy = TransactionBlock.from(transactionBlock);
		if (gasCoin) {
			copy.setGasPayment([gasCoin]);
		}

		copy.setSenderIfNotSet(this.#signer.toSuiAddress());

		return this.#cache.buildTransactionBlock({ transactionBlock: copy });
	};

	executeTransactionBlock(transactionBlock: TransactionBlock | Uint8Array) {
		return this.#queue.runTask(async () => {
			const bytes = isTransactionBlock(transactionBlock)
				? await this.#buildTransactionBlock(transactionBlock)
				: transactionBlock;

			const { signature } = await this.#signer.signTransactionBlock(bytes);
			const results = await this.#cache
				.executeTransactionBlock({
					signature,
					transactionBlock: bytes,
				})
				.catch(async (error) => {
					await this.#cache.reset();
					throw error;
				});

			const effectsBytes = Uint8Array.from(results.rawEffects!);
			const effects = bcs.TransactionEffects.parse(effectsBytes);
			await this.applyEffects(effects);

			return {
				digest: results.digest,
				effects: toB64(effectsBytes),
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
