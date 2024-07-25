// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';

import { bcs } from '../../bcs/index.js';
import type { SuiObjectRef } from '../../bcs/types.js';
import type { SuiClient } from '../../client/index.js';
import type { Signer } from '../../cryptography/index.js';
import type { ObjectCacheOptions } from '../ObjectCache.js';
import { Transaction } from '../Transaction.js';
import { TransactionDataBuilder } from '../TransactionData.js';
import { CachingTransactionExecutor } from './caching.js';
import { ParallelQueue, SerialQueue } from './queue.js';
import { getGasCoinFromEffects } from './serial.js';

const PARALLEL_EXECUTOR_DEFAULTS = {
	coinBatchSize: 20,
	initialCoinBalance: 200_000_000n,
	minimumCoinBalance: 50_000_000n,
	maxPoolSize: 50,
	epochBoundaryWindow: 1_000,
} satisfies Omit<ParallelTransactionExecutorOptions, 'signer' | 'client'>;
export interface ParallelTransactionExecutorOptions extends Omit<ObjectCacheOptions, 'address'> {
	client: SuiClient;
	signer: Signer;
	/** The number of coins to create in a batch when refilling the gas pool */
	coinBatchSize?: number;
	/** The initial balance of each coin created for the gas pool */
	initialCoinBalance?: bigint;
	/** The minimum balance of a coin that can be reused for future transactions.  If the gasCoin is below this value, it will be used when refilling the gasPool */
	minimumCoinBalance?: bigint;
	/** The gasBudget to use if the transaction has not defined it's own gasBudget, defaults to `minimumCoinBalance` */
	defaultGasBudget?: bigint;
	/**
	 * Time to wait before/after the expected epoch boundary before re-fetching the gas pool (in milliseconds).
	 * Building transactions will be paused for up to 2x this duration around each epoch boundary to ensure the
	 * gas price is up-to-date for the next epoch.
	 * */
	epochBoundaryWindow?: number;
	/** The maximum number of transactions that can be execute in parallel, this also determines the maximum number of gas coins that will be created */
	maxPoolSize?: number;
	/** An initial list of coins used to fund the gas pool, uses all owned SUI coins by default */
	sourceCoins?: string[];
}

interface CoinWithBalance {
	id: string;
	version: string;
	digest: string;
	balance: bigint;
}
export class ParallelTransactionExecutor {
	#signer: Signer;
	#client: SuiClient;
	#coinBatchSize: number;
	#initialCoinBalance: bigint;
	#minimumCoinBalance: bigint;
	#epochBoundaryWindow: number;
	#defaultGasBudget: bigint;
	#maxPoolSize: number;
	#sourceCoins: Map<string, SuiObjectRef | null> | null;
	#coinPool: CoinWithBalance[] = [];
	#cache: CachingTransactionExecutor;
	#objectIdQueues = new Map<string, (() => void)[]>();
	#buildQueue = new SerialQueue();
	#executeQueue: ParallelQueue;
	#lastDigest: string | null = null;
	#cacheLock: Promise<void> | null = null;
	#pendingTransactions = 0;
	#gasPrice: null | {
		price: bigint;
		expiration: number;
	} = null;

	constructor(options: ParallelTransactionExecutorOptions) {
		this.#signer = options.signer;
		this.#client = options.client;
		this.#coinBatchSize = options.coinBatchSize ?? PARALLEL_EXECUTOR_DEFAULTS.coinBatchSize;
		this.#initialCoinBalance =
			options.initialCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.initialCoinBalance;
		this.#minimumCoinBalance =
			options.minimumCoinBalance ?? PARALLEL_EXECUTOR_DEFAULTS.minimumCoinBalance;
		this.#defaultGasBudget = options.defaultGasBudget ?? this.#minimumCoinBalance;
		this.#epochBoundaryWindow =
			options.epochBoundaryWindow ?? PARALLEL_EXECUTOR_DEFAULTS.epochBoundaryWindow;
		this.#maxPoolSize = options.maxPoolSize ?? PARALLEL_EXECUTOR_DEFAULTS.maxPoolSize;
		this.#cache = new CachingTransactionExecutor({
			client: options.client,
			cache: options.cache,
		});
		this.#executeQueue = new ParallelQueue(this.#maxPoolSize);
		this.#sourceCoins = options.sourceCoins
			? new Map(options.sourceCoins.map((id) => [id, null]))
			: null;
	}

	resetCache() {
		this.#gasPrice = null;
		return this.#updateCache(() => this.#cache.reset());
	}

	async waitForLastTransaction() {
		await this.#updateCache(() => this.#waitForLastDigest());
	}

	async executeTransaction(transaction: Transaction) {
		const { promise, resolve, reject } = promiseWithResolvers<{
			digest: string;
			effects: string;
		}>();
		const usedObjects = await this.#getUsedObjects(transaction);

		const execute = () => {
			this.#executeQueue.runTask(() => {
				const promise = this.#execute(transaction, usedObjects);

				return promise.then(resolve, reject);
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

	async #getUsedObjects(transaction: Transaction) {
		const usedObjects = new Set<string>();
		let serialized = false;

		transaction.addSerializationPlugin(async (blockData, _options, next) => {
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

		await transaction.prepareForSerialization({ client: this.#client });

		return usedObjects;
	}

	async #execute(transaction: Transaction, usedObjects: Set<string>) {
		let gasCoin!: CoinWithBalance;
		try {
			transaction.setSenderIfNotSet(this.#signer.toSuiAddress());

			await this.#buildQueue.runTask(async () => {
				const data = transaction.getData();

				if (!data.gasData.price) {
					transaction.setGasPrice(await this.#getGasPrice());
				}

				if (!data.gasData.budget) {
					transaction.setGasBudget(this.#defaultGasBudget);
				}

				await this.#updateCache();
				gasCoin = await this.#getGasCoin();
				this.#pendingTransactions++;
				transaction.setGasPayment([
					{
						objectId: gasCoin.id,
						version: gasCoin.version,
						digest: gasCoin.digest,
					},
				]);

				// Resolve cached references
				await this.#cache.buildTransaction({ transaction, onlyTransactionKind: true });
			});

			const bytes = await transaction.build({ client: this.#client });

			const { signature } = await this.#signer.signTransaction(bytes);

			const results = await this.#cache.executeTransaction({
				transaction: bytes,
				signature,
				options: {
					showEffects: true,
				},
			});

			const effectsBytes = Uint8Array.from(results.rawEffects!);
			const effects = bcs.TransactionEffects.parse(effectsBytes);

			const gasResult = getGasCoinFromEffects(effects);
			const gasUsed = effects.V2?.gasUsed;

			if (gasCoin && gasUsed && gasResult.owner === this.#signer.toSuiAddress()) {
				const totalUsed =
					BigInt(gasUsed.computationCost) +
					BigInt(gasUsed.storageCost) +
					BigInt(gasUsed.storageCost) -
					BigInt(gasUsed.storageRebate);

				let usesGasCoin = false;
				new TransactionDataBuilder(transaction.getData()).mapArguments((arg) => {
					if (arg.$kind === 'GasCoin') {
						usesGasCoin = true;
					}

					return arg;
				});

				if (!usesGasCoin && gasCoin.balance >= this.#minimumCoinBalance) {
					this.#coinPool.push({
						id: gasResult.ref.objectId,
						version: gasResult.ref.version,
						digest: gasResult.ref.digest,
						balance: gasCoin.balance - totalUsed,
					});
				} else {
					if (!this.#sourceCoins) {
						this.#sourceCoins = new Map();
					}
					this.#sourceCoins.set(gasResult.ref.objectId, gasResult.ref);
				}
			}

			this.#lastDigest = results.digest;

			return {
				digest: results.digest,
				effects: toB64(effectsBytes),
			};
		} catch (error) {
			if (gasCoin) {
				if (!this.#sourceCoins) {
					this.#sourceCoins = new Map();
				}

				this.#sourceCoins.set(gasCoin.id, null);
			}

			await this.#updateCache(async () => {
				await Promise.all([
					this.#cache.cache.deleteObjects([...usedObjects]),
					this.#waitForLastDigest(),
				]);
			});

			throw error;
		} finally {
			usedObjects.forEach((objectId) => {
				const queue = this.#objectIdQueues.get(objectId);
				if (queue && queue.length > 0) {
					queue.shift()!();
				} else if (queue) {
					this.#objectIdQueues.delete(objectId);
				}
			});
			this.#pendingTransactions--;
		}
	}

	/** Helper for synchronizing cache updates, by ensuring only one update happens at a time.  This can also be used to wait for any pending cache updates  */
	async #updateCache(fn?: () => Promise<void>) {
		if (this.#cacheLock) {
			await this.#cacheLock;
		}

		this.#cacheLock =
			fn?.().then(
				() => {
					this.#cacheLock = null;
				},
				() => {},
			) ?? null;
	}

	async #waitForLastDigest() {
		const digest = this.#lastDigest;
		if (digest) {
			this.#lastDigest = null;
			await this.#client.waitForTransaction({ digest });
		}
	}

	async #getGasCoin() {
		if (this.#coinPool.length === 0 && this.#pendingTransactions <= this.#maxPoolSize) {
			await this.#refillCoinPool();
		}

		if (this.#coinPool.length === 0) {
			throw new Error('No coins available');
		}

		const coin = this.#coinPool.shift()!;
		return coin;
	}

	async #getGasPrice(): Promise<bigint> {
		const remaining = this.#gasPrice
			? this.#gasPrice.expiration - this.#epochBoundaryWindow - Date.now()
			: 0;

		if (remaining > 0) {
			return this.#gasPrice!.price;
		}

		if (this.#gasPrice) {
			const timeToNextEpoch = Math.max(
				this.#gasPrice.expiration + this.#epochBoundaryWindow - Date.now(),
				1_000,
			);

			await new Promise((resolve) => setTimeout(resolve, timeToNextEpoch));
		}

		const state = await this.#client.getLatestSuiSystemState();

		this.#gasPrice = {
			price: BigInt(state.referenceGasPrice),
			expiration:
				Number.parseInt(state.epochStartTimestampMs, 10) +
				Number.parseInt(state.epochDurationMs, 10),
		};

		return this.#getGasPrice();
	}

	async #refillCoinPool() {
		const batchSize = Math.min(
			this.#coinBatchSize,
			this.#maxPoolSize - (this.#coinPool.length + this.#pendingTransactions) + 1,
		);

		if (batchSize === 0) {
			return;
		}

		const txb = new Transaction();
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

		await this.waitForLastTransaction();

		const result = await this.#client.signAndExecuteTransaction({
			transaction: txb,
			signer: this.#signer,
			options: {
				showRawEffects: true,
			},
		});

		const effects = bcs.TransactionEffects.parse(Uint8Array.from(result.rawEffects!));
		effects.V2?.changedObjects.forEach(([id, { outputState }], i) => {
			if (i === effects.V2?.gasObjectIndex || !outputState.ObjectWrite) {
				return;
			}

			this.#coinPool.push({
				id,
				version: effects.V2!.lamportVersion,
				digest: outputState.ObjectWrite[0],
				balance: BigInt(this.#initialCoinBalance),
			});
		});

		if (!this.#sourceCoins) {
			this.#sourceCoins = new Map();
		}

		const gasObject = getGasCoinFromEffects(effects).ref;
		this.#sourceCoins!.set(gasObject.objectId, gasObject);

		await this.#client.waitForTransaction({ digest: result.digest });
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
