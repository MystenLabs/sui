// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiObjectRef } from '../../bcs/types.js';
import type { SuiClient, SuiTransactionBlockResponse } from '../../client/index.js';
import type { Signer } from '../../cryptography/index.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { TransactionBlock } from '../TransactionBlock.js';
import { CachingTransactionBlockExecutor } from './caching.js';

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
	#sourceCoinIds: string[] | null = null;
	#sourceCoins: SuiObjectRef[] | null = null;
	#coinPool = new Set<CoinWithBalance>();
	#cache: CachingTransactionBlockExecutor;
	#objectIdQueues = new Map<string, (() => void)[]>();
	#refillPromise: Promise<void> | null = null;

	constructor(options: ParallelExecutorOptions) {
		this.#signer = options.signer;
		this.#client = options.client;
		this.#coinBatchSize = options.coinBatchSize ?? PARALLEL_EXECUTOR_DEFAULTS.coinBatchSize;
		this.#initialCoinBalance =
			options.initialCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.initialCoinBalance;
		this.#minimumCoinBalance =
			options.minimumCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.minimumCoinBalance;
		this.#maxPoolSize = options.maxPoolSize ?? PARALLEL_EXECUTOR_DEFAULTS.maxPoolSize;
		this.#sourceCoinIds = options.sourceCoins ? options.sourceCoins : null;
		this.#cache = new CachingTransactionBlockExecutor({
			address: this.#signer.toSuiAddress(),
			client: options.client,
			cache: options.cache,
		});
	}

	async executeTransactionBlock(transactionBlock: TransactionBlock) {
		const { promise, resolve, reject } = promiseWithResolvers<SuiTransactionBlockResponse>();
		const usedObjects = new Set<string>();
		transactionBlock.addSerializationPlugin(async (blockData, _options, next) => {
			await next();

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
			const bytes = await this.#runSequentialTask(() =>
				this.#cache.buildTransactionBlock({ transactionBlock }),
			);

			const { signature } = await this.#signer.signTransactionBlock(bytes);

			await this.#runParallelTask(async () => {
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
						} else if (this.#sourceCoins) {
							this.#sourceCoins.push({
								objectId: gasCoin.id,
								version: gasCoin.version,
								digest: gasCoin.digest,
							});
						} else if (this.#sourceCoinIds) {
							this.#sourceCoinIds.push(gasCoin.id);
						} else {
							this.#sourceCoins = [
								{
									objectId: gasCoin.id,
									version: gasCoin.version,
									digest: gasCoin.digest,
								},
							];
						}
					}

					resolve(results);
				} catch (error) {
					if (gasCoin) {
						// Coin might have been used to pay for gas of failed transaction
						// Add it to the list of source coins and throw out the versions/digests
						if (!this.#sourceCoinIds) {
							this.#sourceCoinIds = [gasCoin.id];
							this.#sourceCoins?.forEach((coin) => {
								this.#sourceCoinIds!.push(coin.objectId);
							});
							this.#sourceCoins = null;
						}
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
		[...usedObjects].forEach((objectId) => {
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

	#sequentialQueue: (() => Promise<void>)[] = [];
	async #runSequentialTask<T>(task: () => Promise<T>): Promise<T> {
		return new Promise((resolve, reject) => {
			this.#sequentialQueue.push(async () => {
				const promise = task();
				promise.then(resolve, reject);

				promise.finally(() => {
					this.#sequentialQueue.shift();
					if (this.#sequentialQueue.length > 0) {
						this.#sequentialQueue[0]();
					}
				});
			});

			if (this.#sequentialQueue.length === 1) {
				this.#sequentialQueue[0]();
			}
		});
	}

	#activeTasks = 0;
	#parallelQueue: (() => Promise<void>)[] = [];
	#runParallelTask<T>(task: () => Promise<T>): Promise<T> {
		return new Promise<T>((resolve, reject) => {
			if (this.#activeTasks < this.#maxPoolSize) {
				this.#activeTasks++;

				const promise = task().then(resolve, reject);

				promise.finally(() => {
					if (this.#parallelQueue.length > 0) {
						this.#parallelQueue.shift()!();
					} else {
						this.#activeTasks--;
					}
				});
			} else {
				this.#parallelQueue.push(async () => {
					try {
						const result = await task();
						resolve(result);
					} catch (error) {
						reject(error);
					} finally {
						this.#parallelQueue.shift();
						if (this.#parallelQueue.length > 0) {
							this.#parallelQueue.shift()!();
						} else {
							this.#activeTasks--;
						}
					}
				});
			}
		});
	}

	async #getGasCoin() {
		if (this.#coinPool.size === 0 && this.#activeTasks < this.#maxPoolSize) {
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
			this.#maxPoolSize - (this.#coinPool.size + this.#activeTasks),
		);

		if (batchSize === 0) {
			return;
		}

		const txb = new TransactionBlock();
		const address = this.#signer.toSuiAddress();
		txb.setSender(address);

		if (this.#sourceCoinIds) {
			const coins = await this.#client.multiGetObjects({
				ids: this.#sourceCoinIds,
			});

			const payment = coins
				.filter((coin): coin is typeof coin & { data: object } => coin.data !== null)
				.map(({ data }) => ({
					objectId: data.objectId,
					version: data.version,
					digest: data.digest,
				}));

			txb.setGasPayment(payment);
			this.#sourceCoinIds = null;
			this.#sourceCoins = [];
		} else if (this.#sourceCoins) {
			txb.setGasPayment(this.#sourceCoins);
			this.#sourceCoins = [];
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

		if (!this.#sourceCoins) {
			this.#sourceCoins = [];
		}

		this.#sourceCoins.push(result.effects!.gasObject.reference);
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
