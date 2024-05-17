// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../../bcs/index.js';
import type { ExecuteTransactionBlockParams, SuiClient } from '../../client/index.js';
import type { Signer } from '../../cryptography/keypair.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import type { TransactionBlock } from '../TransactionBlock.js';
import { isTransactionBlock } from '../TransactionBlock.js';
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

		const gasCoin = getGasCoinFromEffects(effects);
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

		if (gasCoin) {
			transactionBlock.setGasPayment([gasCoin]);
		}

		transactionBlock.setSenderIfNotSet(this.#signer.toSuiAddress());

		return this.#cache.buildTransactionBlock({ transactionBlock });
	};

	executeTransactionBlock({
		transactionBlock,
		...input
	}: {
		transactionBlock: TransactionBlock | Uint8Array;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock' | 'signature'>) {
		return this.#queue.runTask(async () => {
			const bytes = isTransactionBlock(transactionBlock)
				? await this.#buildTransactionBlock(transactionBlock)
				: transactionBlock;

			const { signature } = await this.#signer.signTransactionBlock(bytes);
			const results = await this.#cache.executeTransactionBlock({
				...input,
				signature,
				transactionBlock: bytes,
			});

			const effects = bcs.TransactionEffects.parse(Uint8Array.from(results.rawEffects!));
			await this.applyEffects(effects);

			return results;
		});
	}
}

function getGasCoinFromEffects(effects: typeof bcs.TransactionEffects.$inferType) {
	if (!effects.V2) {
		return null;
	}

	const gasObjectChange = effects.V2.changedObjects[effects.V2.gasObjectIndex!];

	if (!gasObjectChange) {
		return null;
	}

	const [objectId, { outputState }] = gasObjectChange;

	if (!outputState.ObjectWrite) {
		return null;
	}

	const [digest] = outputState.ObjectWrite;

	return {
		objectId,
		digest,
		version: effects.V2.lamportVersion,
	};
}
