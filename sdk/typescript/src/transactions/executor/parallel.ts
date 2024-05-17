// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiObjectRef } from '../../bcs/types.js';
import type { SuiClient, SuiTransactionBlockResponse } from '../../client/index.js';
import type { Signer } from '../../cryptography/index.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { TransactionBlock } from '../TransactionBlock.js';
import { CachingTransactionBlockExecutor } from './caching.js';
import { ParallelQueue, SerialQueue } from './queue.js';

const PARALLEL_EXECUTOR_DEFAULTS = {
	coinBatchSize: 20,
	initialCoinBalance: 200_000_000n,
	minimumCoinBalance: 50_000_000n,
	maxPoolSize: 50,
} satisfies Omit<ParallelExecutorOptions, 'signer' | 'client'>;
export interface ParallelExecutorOptions extends Omit<ObjectCacheOptions, 'address'> {
	client: SuiClient;
	signer: Signer;
	coinBatchSize?: number;
	initialCoinBalance?: bigint;
	minimumCoinBalance?: bigint;
	maxPoolSize?: number;
	sourceCoins?: string[];
}

interface CoinWithBalance {
	id: string;
	version: string;
	digest: string;
	balance: bigint;
}
export class ParallelExecutor {
	#signer: Signer;
	#client: SuiClient;
	#coinBatchSize: number;
	#initialCoinBalance: bigint;
	#minimumCoinBalance: bigint;
	#maxPoolSize: number;
	#sourceCoins: Map<string, SuiObjectRef | null> | null;
	#coinPool = new Set<CoinWithBalance>();
	#cache: CachingTransactionBlockExecutor;
	#objectIdQueues = new Map<string, (() => void)[]>();
	#refillPromise: Promise<void> | null = null;
	#buildQueue = new SerialQueue();
	#executeQueue: ParallelQueue;

	constructor(options: ParallelExecutorOptions) {
		this.#signer = options.signer;
		this.#client = options.client;
		this.#coinBatchSize = options.coinBatchSize ?? PARALLEL_EXECUTOR_DEFAULTS.coinBatchSize;
		this.#initialCoinBalance =
			options.initialCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.initialCoinBalance;
		this.#minimumCoinBalance =
			options.minimumCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.minimumCoinBalance;
		this.#maxPoolSize = options.maxPoolSize ?? PARALLEL_EXECUTOR_DEFAULTS.maxPoolSize;
		this.#cache = new CachingTransactionBlockExecutor({
			address: this.#signer.toSuiAddress(),
			client: options.client,
			cache: options.cache,
		});
		this.#executeQueue = new ParallelQueue(this.#maxPoolSize);
		this.#sourceCoins = new Map(options.sourceCoins?.map((id) => [id, null]));
	}

	async executeTransactionBlock(transactionBlock: TransactionBlock) {
		const { promise, resolve, reject } = promiseWithResolvers<SuiTransactionBlockResponse>();
		const usedObjects = new Set<string>();
		let serialized = false;
		transactionBlock.addSerializationPlugin(async (blockData, _options, next) => {
			await next();

			if (serialized) {
				return;
			}
			serialized = true;

			blockData.inputs.forEach((input) => {
				if (input.Object?.ImmOrOwnedObject?.objectId) {
					usedObjects.add(input.Object.ImmOrOwnedObject.objectId);
				} else if (input.Object?.Receiving?.objectId) {
					usedObjects.add(input.Object.Receiving.objectId);
				} else if (
					input.UnresolvedObject?.objectId &&
					!input.UnresolvedObject.initialSharedVersion
				) {
					usedObjects.add(input.UnresolvedObject.objectId);
				}
			});
		});

		await transactionBlock.prepareForSerialization({ client: this.#client });

		const execute = async () => {
			transactionBlock.setSenderIfNotSet(this.#signer.toSuiAddress());
			const bytes = await this.#buildQueue.runTask(() =>
				this.#cache.buildTransactionBlock({ transactionBlock }),
			);

			const { signature } = await this.#signer.signTransactionBlock(bytes);

			await this.#executeQueue.runTask(async () => {
				let gasCoin: CoinWithBalance | null = null;
				try {
					gasCoin = await this.#getGasCoin();
					transactionBlock.setGasPayment([]);

					const results = await this.#cache.executeTransactionBlock({
						transactionBlock: bytes,
						signature,
						options: {
							showEffects: true,
						},
					});

					const gasOwner = results.effects?.gasObject.owner;
					const gasUsed = results.effects?.gasUsed;

					if (
						gasCoin &&
						gasUsed &&
						gasOwner &&
						typeof gasOwner === 'object' &&
						'AddressOwner' in gasOwner &&
						gasOwner.AddressOwner === this.#signer.toSuiAddress()
					) {
						const totalUsed =
							BigInt(gasUsed.computationCost) +
							BigInt(gasUsed.storageCost) +
							BigInt(gasUsed.storageCost) -
							BigInt(gasUsed.storageRebate);
						gasCoin.balance -= totalUsed;

						if (gasCoin.balance >= this.#minimumCoinBalance) {
							this.#coinPool.add(gasCoin);
						} else {
							if (!this.#sourceCoins) {
								this.#sourceCoins = new Map();
							}
							this.#sourceCoins.set(gasCoin.id, {
								objectId: gasCoin.id,
								version: gasCoin.version,
								digest: gasCoin.digest,
							});
						}
					}

					resolve(results);
				} catch (error) {
					if (gasCoin) {
						if (!this.#sourceCoins) {
							this.#sourceCoins = new Map();
						}

						this.#sourceCoins.set(gasCoin.id, null);
					}
					reject(error);
				} finally {
					usedObjects.forEach((objectId) => {
						const queue = this.#objectIdQueues.get(objectId);
						if (queue && queue.length > 0) {
							queue.shift()!();
						} else if (queue) {
							this.#objectIdQueues.delete(objectId);
						}
					});
				}
			});
		};

		const conflicts = new Set<string>();

		usedObjects.forEach((objectId) => {
			const queue = this.#objectIdQueues.get(objectId);
			if (queue) {
				conflicts.add(objectId);
				this.#objectIdQueues.get(objectId)!.push(() => {
					conflicts.delete(objectId);
					if (conflicts.size === 0) {
						execute();
					}
				});
			} else {
				this.#objectIdQueues.set(objectId, []);
			}
		});

		if (conflicts.size === 0) {
			execute();
		}

		return promise;
	}

	async #getGasCoin() {
		if (this.#coinPool.size === 0 && this.#executeQueue.activeTasks < this.#maxPoolSize) {
			await this.#refillCoinPool();
		}

		if (this.#coinPool.size === 0) {
			throw new Error('No coins available');
		}

		const coin = this.#coinPool.values().next().value as CoinWithBalance;
		this.#coinPool.delete(coin);
		return coin;
	}

	#refillCoinPool() {
		if (!this.#refillPromise) {
			this.#refillPromise = this.#createRefillCoinPoolPromise();
		}

		return this.#refillPromise;
	}

	async #createRefillCoinPoolPromise() {
		if (this.#refillPromise) {
			return this.#refillPromise;
		}
		const batchSize = Math.min(
			this.#coinBatchSize,
			this.#maxPoolSize - (this.#coinPool.size + this.#executeQueue.activeTasks),
		);

		if (batchSize === 0) {
			return;
		}

		const txb = new TransactionBlock();
		const address = this.#signer.toSuiAddress();
		txb.setSender(address);

		if (this.#sourceCoins) {
			const refs = [];
			const ids = [];
			for (const [id, ref] of this.#sourceCoins) {
				if (ref) {
					refs.push(ref);
				} else {
					ids.push(id);
				}
			}

			if (ids.length > 0) {
				const coins = await this.#client.multiGetObjects({
					ids,
				});
				refs.push(
					...coins
						.filter((coin): coin is typeof coin & { data: object } => coin.data !== null)
						.map(({ data }) => ({
							objectId: data.objectId,
							version: data.version,
							digest: data.digest,
						})),
				);
			}

			txb.setGasPayment(refs);
			this.#sourceCoins = new Map();
		}

		const amounts = new Array(batchSize).fill(this.#initialCoinBalance);
		const results = txb.splitCoins(txb.gas, amounts);
		const coinResults = [];
		for (let i = 0; i < amounts.length; i++) {
			coinResults.push(results[i]);
		}
		txb.transferObjects(coinResults, address);

		const result = await this.#client.signAndExecuteTransactionBlock({
			transactionBlock: txb,
			signer: this.#signer,
			options: {
				showEffects: true,
				showObjectChanges: true,
			},
		});

		result.objectChanges?.forEach((change) => {
			if (
				change.type === 'created' &&
				change.objectId !== result.effects?.gasObject.reference.objectId
			) {
				this.#coinPool.add({
					id: change.objectId,
					version: change.version,
					digest: change.digest,
					balance: BigInt(this.#initialCoinBalance),
				});
			}
		});

		this.#sourceCoins!.set(
			result.effects!.gasObject.reference.objectId,
			result.effects!.gasObject.reference,
		);
	}
}

function promiseWithResolvers<T>() {
	let resolve: (value: T) => void;
	let reject: (reason: any) => void;

	const promise = new Promise<T>((_resolve, _reject) => {
		resolve = _resolve;
		reject = _reject;
	});

	return { promise, resolve: resolve!, reject: reject! };
}
