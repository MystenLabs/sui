// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '../bcs/index.js';
import type { SuiClient } from '../client/client.js';
import type { ExecuteTransactionBlockParams } from '../client/index.js';
import type { Signer } from '../cryptography/keypair.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { OpenMoveTypeSignature } from './data/internal.js';
import type { TransactionPlugin } from './json-rpc-resolver.js';
import type { Transaction } from './Transaction.js';

export interface ObjectCacheEntry {
	objectId: string;
	version: string;
	digest: string;
	owner: string | null;
	initialSharedVersion: string | null;
}

export interface MoveFunctionCacheEntry {
	package: string;
	module: string;
	function: string;
	parameters: OpenMoveTypeSignature[];
}

export interface CacheEntryTypes {
	OwnedObject: ObjectCacheEntry;
	SharedOrImmutableObject: ObjectCacheEntry;
	MoveFunction: MoveFunctionCacheEntry;
}
export abstract class AsyncCache {
	protected abstract get<T extends keyof CacheEntryTypes>(
		type: T,
		key: string,
	): Promise<CacheEntryTypes[T] | null>;
	protected abstract set<T extends keyof CacheEntryTypes>(
		type: T,
		key: string,
		value: CacheEntryTypes[T],
	): Promise<void>;
	protected abstract delete<T extends keyof CacheEntryTypes>(type: T, key: string): Promise<void>;
	abstract clear<T extends keyof CacheEntryTypes>(type?: T): Promise<void>;

	async getObject(id: string) {
		const [owned, shared] = await Promise.all([
			this.get('OwnedObject', id),
			this.get('SharedOrImmutableObject', id),
		]);

		return owned ?? shared ?? null;
	}

	async getObjects(ids: string[]) {
		return Promise.all([...ids.map((id) => this.getObject(id))]);
	}

	async addObject(object: ObjectCacheEntry) {
		if (object.owner) {
			await this.set('OwnedObject', object.objectId, object);
		} else {
			await this.set('SharedOrImmutableObject', object.objectId, object);
		}

		return object;
	}

	async deleteObject(id: string) {
		await Promise.all([
			await this.delete('OwnedObject', id),
			await this.delete('SharedOrImmutableObject', id),
		]);
	}

	async getMoveFunctionDefinition(ref: { package: string; module: string; function: string }) {
		const functionName = `${normalizeSuiAddress(ref.package)}::${ref.module}::${ref.function}`;
		return this.get('MoveFunction', functionName);
	}

	async addMoveFunctionDefinition(functionEntry: MoveFunctionCacheEntry) {
		const pkg = normalizeSuiAddress(functionEntry.package);
		const functionName = `${pkg}::${functionEntry.module}::${functionEntry.function}`;
		const entry = {
			...functionEntry,
			package: pkg,
		};

		await this.set('MoveFunction', functionName, entry);

		return entry;
	}

	async deleteMoveFunctionDefinition(ref: { package: string; module: string; function: string }) {
		const functionName = `${normalizeSuiAddress(ref.package)}::${ref.module}::${ref.function}`;
		await this.delete('MoveFunction', functionName);
	}
}

export class InMemoryCache extends AsyncCache {
	#caches = {
		OwnedObject: new Map<string, ObjectCacheEntry>(),
		SharedOrImmutableObject: new Map<string, ObjectCacheEntry>(),
		MoveFunction: new Map<string, MoveFunctionCacheEntry>(),
	};

	protected async get<T extends keyof CacheEntryTypes>(type: T, key: string) {
		return (this.#caches[type].get(key) as CacheEntryTypes[T]) ?? null;
	}

	protected async set<T extends keyof CacheEntryTypes>(
		type: T,
		key: string,
		value: CacheEntryTypes[T],
	) {
		(this.#caches[type] as Map<string, typeof value>).set(key, value as never);
	}

	protected async delete<T extends keyof CacheEntryTypes>(type: T, key: string) {
		this.#caches[type].delete(key);
	}

	async clear<T extends keyof CacheEntryTypes>(type?: T) {
		if (type) {
			this.#caches[type].clear();
		} else {
			for (const cache of Object.values(this.#caches)) {
				cache.clear();
			}
		}
	}
}

interface ObjectCacheOptions {
	cache?: AsyncCache;
	address: string;
}

export class ObjectCache {
	#cache: AsyncCache;
	#address: string;

	constructor({ cache = new InMemoryCache(), address }: ObjectCacheOptions) {
		this.#cache = cache;
		this.#address = normalizeSuiAddress(address);
	}

	asPlugin(): TransactionPlugin {
		return async (transactionData, _options, next) => {
			const unresolvedObjects = transactionData.inputs
				.filter((input) => input.UnresolvedObject)
				.map((input) => input.UnresolvedObject!.objectId);

			const cached = (await this.#cache.getObjects(unresolvedObjects)).filter(
				(obj) => obj !== null,
			);

			const byId = new Map(cached.map((obj) => [obj!.objectId, obj]));

			for (const input of transactionData.inputs) {
				if (!input.UnresolvedObject) {
					continue;
				}

				const cached = byId.get(input.UnresolvedObject.objectId);

				if (!cached) {
					continue;
				}

				if (cached.initialSharedVersion && !input.UnresolvedObject.initialSharedVersion) {
					input.UnresolvedObject.initialSharedVersion = cached.initialSharedVersion;
				}

				if (cached.version && !input.UnresolvedObject.version) {
					input.UnresolvedObject.version = cached.version;
				}

				if (cached.digest && !input.UnresolvedObject.digest) {
					input.UnresolvedObject.digest = cached.digest;
				}
			}

			await Promise.all(
				transactionData.commands.map(async (commands) => {
					if (commands.MoveCall) {
						const def = await this.getMoveFunctionDefinition({
							package: commands.MoveCall.package,
							module: commands.MoveCall.module,
							function: commands.MoveCall.function,
						});

						if (def) {
							commands.MoveCall._argumentTypes = def.parameters;
						}
					}
				}),
			);

			await next();

			await Promise.all(
				transactionData.commands.map(async (commands) => {
					if (commands.MoveCall?._argumentTypes) {
						await this.#cache.addMoveFunctionDefinition({
							package: commands.MoveCall.package,
							module: commands.MoveCall.module,
							function: commands.MoveCall.function,
							parameters: commands.MoveCall._argumentTypes,
						});
					}
				}),
			);
		};
	}

	async clear() {
		await this.#cache.clear();
	}

	async getMoveFunctionDefinition(ref: { package: string; module: string; function: string }) {
		return this.#cache.getMoveFunctionDefinition(ref);
	}

	async getObjects(ids: string[]) {
		return this.#cache.getObjects(ids);
	}

	async clearOwnedObjects() {
		await this.#cache.clear('OwnedObject');
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		if (!effects.V2) {
			throw new Error(`Unsupported transaction effects version ${effects.$kind}`);
		}

		const { lamportVersion, changedObjects } = effects.V2;

		await Promise.all(
			changedObjects.map(async ([id, change]) => {
				if (change.outputState.NotExist) {
					await this.#cache.deleteObject(id);
				} else if (change.outputState.ObjectWrite) {
					const [digest, owner] = change.outputState.ObjectWrite;

					// Remove objects not owned by address after transaction
					if (owner.ObjectOwner || (owner.AddressOwner && owner.AddressOwner !== this.#address)) {
						await this.#cache.deleteObject(id);
					}

					await this.#cache.addObject({
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

export class CachingTransactionExecutor {
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
	}

	async buildTransaction({ transaction }: { transaction: Transaction }) {
		return transaction.build({
			client: this.#client,
		});
	}

	async executeTransaction({
		transaction,
		options,
		...input
	}: {
		transaction: Transaction;
	} & Omit<ExecuteTransactionBlockParams, 'transactionBlock'>) {
		transaction.addSerializationPlugin(this.cache.asPlugin());
		const results = await this.#client.executeTransactionBlock({
			...input,
			transactionBlock: await transaction.build({
				client: this.#client,
			}),
			options: {
				...options,
				showRawEffects: true,
			},
		});

		if (results.rawEffects) {
			const effects = bcs.TransactionEffects.parse(Uint8Array.from(results.rawEffects));
			await this.cache.applyEffects(effects);
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
		transaction.addBuildPlugin(this.cache.asPlugin());
		const results = await this.#client.signAndExecuteTransaction({
			...input,
			transaction: await transaction.build({
				client: this.#client,
			}),
			options: {
				...options,
				showRawEffects: true,
			},
		});

		if (results.rawEffects) {
			const effects = bcs.TransactionEffects.parse(Uint8Array.from(results.rawEffects));
			await this.cache.applyEffects(effects);
		}

		return results;
	}
}
