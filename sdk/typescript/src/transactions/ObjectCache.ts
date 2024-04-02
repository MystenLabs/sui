// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../bcs/index.js';
import type { SuiClient } from '../client/client.js';
import type { ExecuteTransactionBlockParams } from '../client/index.js';
import type { Signer } from '../cryptography/keypair.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { OpenMoveTypeSignature } from './blockData/v2.js';
import type { TransactionBlock } from './TransactionBlock.js';
import type { TransactionBlockDataResolverPlugin } from './TransactionBlockDataResolver.js';

export interface AsyncCache<T> {
	get(key: string): Promise<T | null>;
	set(key: string, value: T): Promise<void>;
	delete(key: string): Promise<void>;
	clear(): Promise<void>;
}

interface ObjectCacheEntry {
	objectId: string;
	version: string;
	digest: string;
	owner: string | null;
	initialSharedVersion: string | null;
}

interface MoveFunctionEntry {
	package: string;
	module: string;
	function: string;
	parameters: OpenMoveTypeSignature[];
}

class InMemoryCache<T> implements AsyncCache<T> {
	#cache = new Map<string, T>();

	async get(key: string): Promise<T | null> {
		return this.#cache.get(key) ?? null;
	}

	async set(key: string, value: T): Promise<void> {
		this.#cache.set(key, value);
	}

	async delete(key: string): Promise<void> {
		this.#cache.delete(key);
	}

	async clear(): Promise<void> {
		this.#cache.clear();
	}

	static createCache<T>(): AsyncCache<T> {
		return new InMemoryCache<T>();
	}
}

interface ObjectCacheOptions {
	createCache?: <T>(name: string) => AsyncCache<T>;
}

export class ObjectCache implements TransactionBlockDataResolverPlugin {
	#createCache: <T>(name: string) => AsyncCache<T>;
	#objects: AsyncCache<ObjectCacheEntry>;
	#functions: AsyncCache<MoveFunctionEntry>;

	constructor({ createCache = InMemoryCache.createCache }: ObjectCacheOptions = {}) {
		this.#createCache = createCache;
		this.#objects = this.#createCache('objects');
		this.#functions = this.#createCache('functions');
	}

	getObjects: NonNullable<TransactionBlockDataResolverPlugin['getObjects']> = async (ids, next) => {
		const results = new Map<string, ObjectCacheEntry>();

		await Promise.all(
			ids.map(async (id) => {
				const cachedObject = await this.#objects.get(id);

				if (cachedObject) {
					results.set(id, cachedObject);
				}
			}),
		);
		const missingIds = ids.filter((id) => !results.has(id));

		if (missingIds.length > 0) {
			const newObjects = await next(missingIds);

			await Promise.all(
				newObjects.map(async (newObject) => {
					await this.addObject(newObject);
					results.set(newObject.objectId, newObject);
				}),
			);
		}

		return ids.map((id) => results.get(id)!);
	};

	getMoveFunctionDefinition: NonNullable<
		TransactionBlockDataResolverPlugin['getMoveFunctionDefinition']
	> = async (ref, next) => {
		const functionName = `${normalizeSuiAddress(ref.package)}::${ref.module}::${ref.function}`;
		const cached = await this.#functions.get(functionName);
		if (cached) {
			return cached;
		}

		const functionDefinition = await next(ref);

		return await this.addFunction(functionDefinition);
	};

	async clearCache() {
		await Promise.all([this.#objects.clear(), this.#functions.clear()]);
	}

	async addObject(object: ObjectCacheEntry) {
		await this.#objects.set(object.objectId, object);
		return object;
	}

	async addFunction(functionEntry: MoveFunctionEntry) {
		const pkg = normalizeSuiAddress(functionEntry.package);
		const functionName = `${pkg}::${functionEntry.module}::${functionEntry.function}`;
		const entry = {
			...functionEntry,
			package: pkg,
		};

		await this.#functions.set(functionName, entry);

		return entry;
	}

	async invalidateObject(id: string) {
		await this.#objects.delete(id);
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		if (!effects.V2) {
			throw new Error(`Unsupported transaction effects version ${effects.$kind}`);
		}

		const { lamportVersion, changedObjects } = effects.V2;

		await Promise.all(
			changedObjects.map(async ([id, change]) => {
				if (change.outputState.NotExist) {
					await this.invalidateObject(id);
				} else if (change.outputState.ObjectWrite) {
					const [digest, owner] = change.outputState.ObjectWrite;

					await this.addObject({
						objectId: id,
						digest,
						version: lamportVersion,
						owner: owner.AddressOwner ?? owner.ObjectOwner ?? null,
						initialSharedVersion: owner.Shared?.initialSharedVersion ?? null,
					});
				}
			}),
		);
	}
}

export class CachingTransactionBlockExecutor extends ObjectCache {
	#client: SuiClient;
	constructor(client: SuiClient, options?: ObjectCacheOptions) {
		super(options);
		this.#client = client;
	}

	buildTransactionBlock({
		transactionBlock,
		dataResolvers,
	}: {
		transactionBlock: TransactionBlock;
		dataResolvers?: TransactionBlockDataResolverPlugin[];
	}) {
		return transactionBlock.build({
			client: this.#client,
			dataResolvers: [this, ...(dataResolvers ?? [])],
		});
	}

	async executeTransactionBlock({
		transactionBlock,
		dataResolvers,
		options,
		...input
	}: {
		transactionBlock: TransactionBlock;
		dataResolvers?: TransactionBlockDataResolverPlugin[];
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock'>) {
		const results = await this.#client.executeTransactionBlock({
			...input,
			transactionBlock: await transactionBlock.build({
				client: this.#client,
				dataResolvers: [this, ...(dataResolvers ?? [])],
			}),
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
		dataResolvers,
		...input
	}: {
		transactionBlock: TransactionBlock;
		dataResolvers?: TransactionBlockDataResolverPlugin[];
		signer: Signer;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock' | 'signature'>) {
		const results = await this.#client.signAndExecuteTransactionBlock({
			...input,
			transactionBlock: await transactionBlock.build({
				client: this.#client,
				dataResolvers: [this, ...(dataResolvers ?? [])],
			}),
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
}
