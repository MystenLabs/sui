// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { bcs } from '../../bcs/index.js';
import type { ExecuteTransactionBlockParams } from '../../client/index.js';
import type { TransactionBlock } from '../TransactionBlock.js';
import { isTransactionBlock } from '../TransactionBlock.js';
import { CachingTransactionBlockExecutor } from './caching.js';

export class SerialTransactionBlockExecutor extends CachingTransactionBlockExecutor {
	#queue: (() => Promise<void>)[] = [];

	async #runTask<T>(task: () => Promise<T>): Promise<T> {
		return new Promise((resolve, reject) => {
			this.#queue.push(async () => {
				const promise = task();
				promise.then(resolve, reject);

				promise.finally(() => {
					this.#queue.shift();
					if (this.#queue.length > 0) {
						this.#queue[0]();
					}
				});
			});

			if (this.#queue.length === 1) {
				this.#queue[0]();
			}
		});
	}

	override async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		if (!effects.V2) {
			return;
		}

		await super.applyEffects(effects);

		const gasCoin = getGasCoinFromEffects(effects);
		if (gasCoin) {
			this.cache.setCustom('gasCoin', gasCoin);
		} else {
			this.cache.deleteCustom('gasCoin');
		}
	}

	override async buildTransactionBlock(input: { transactionBlock: TransactionBlock }) {
		return this.#runTask(async () => this.#buildTransactionBlock(input));
	}

	#buildTransactionBlock = async (input: { transactionBlock: TransactionBlock }) => {
		const gasCoin = await this.cache.getCustom<{
			objectId: string;
			version: string;
			digest: string;
		}>('gasCoin');

		if (gasCoin) {
			input.transactionBlock.setGasPayment([gasCoin]);
		}

		return super.buildTransactionBlock(input);
	};

	override async executeTransactionBlock({
		transactionBlock,
		...input
	}: {
		transactionBlock: TransactionBlock | Uint8Array;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock'>) {
		return this.#runTask(async () =>
			super.executeTransactionBlock({
				...input,
				transactionBlock: isTransactionBlock(transactionBlock)
					? await this.#buildTransactionBlock({ transactionBlock })
					: transactionBlock,
			}),
		);
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
