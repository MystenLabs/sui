// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { bcs } from '../bcs/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { OpenMoveTypeSignature } from './data/internal.js';
import type { TransactionPlugin } from './json-rpc-resolver.js';

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
	Custom: unknown;
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

	async addObjects(objects: ObjectCacheEntry[]) {
		await Promise.all(objects.map(async (object) => this.addObject(object)));
	}

	async deleteObject(id: string) {
		await Promise.all([this.delete('OwnedObject', id), this.delete('SharedOrImmutableObject', id)]);
	}

	async deleteObjects(ids: string[]) {
		await Promise.all(ids.map((id) => this.deleteObject(id)));
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

	async getCustom<T>(key: string) {
		return this.get('Custom', key) as Promise<T | null>;
	}

	async setCustom<T>(key: string, value: T) {
		return this.set('Custom', key, value);
	}

	async deleteCustom(key: string) {
		return this.delete('Custom', key);
	}
}

export class InMemoryCache extends AsyncCache {
	#caches = {
		OwnedObject: new Map<string, ObjectCacheEntry>(),
		SharedOrImmutableObject: new Map<string, ObjectCacheEntry>(),
		MoveFunction: new Map<string, MoveFunctionCacheEntry>(),
		Custom: new Map<string, unknown>(),
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

export interface ObjectCacheOptions {
	cache?: AsyncCache;
}

export class ObjectCache {
	#cache: AsyncCache;

	constructor({ cache = new InMemoryCache() }: ObjectCacheOptions) {
		this.#cache = cache;
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
				} else {
					if (cached.version && !input.UnresolvedObject.version) {
						input.UnresolvedObject.version = cached.version;
					}

					if (cached.digest && !input.UnresolvedObject.digest) {
						input.UnresolvedObject.digest = cached.digest;
					}
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

	async deleteObjects(ids: string[]) {
		return this.#cache.deleteObjects(ids);
	}

	async clearOwnedObjects() {
		await this.#cache.clear('OwnedObject');
	}

	async clearCustom() {
		await this.#cache.clear('Custom');
	}

	async getCustom<T>(key: string) {
		return this.#cache.getCustom<T>(key);
	}

	async setCustom<T>(key: string, value: T) {
		return this.#cache.setCustom(key, value);
	}

	async deleteCustom(key: string) {
		return this.#cache.deleteCustom(key);
	}

	async applyEffects(effects: typeof bcs.TransactionEffects.$inferType) {
		if (!effects.V2) {
			throw new Error(`Unsupported transaction effects version ${effects.$kind}`);
		}

		const { lamportVersion, changedObjects } = effects.V2;

		const deletedIds: string[] = [];
		const addedObjects: ObjectCacheEntry[] = [];

		changedObjects.map(async ([id, change]) => {
			if (change.outputState.NotExist) {
				await this.#cache.deleteObject(id);
			} else if (change.outputState.ObjectWrite) {
				const [digest, owner] = change.outputState.ObjectWrite;

				addedObjects.push({
					objectId: id,
					digest,
					version: lamportVersion,
					owner: owner.AddressOwner ?? owner.ObjectOwner ?? null,
					initialSharedVersion: owner.Shared?.initialSharedVersion ?? null,
				});
			}
		});

		await Promise.all([
			this.#cache.addObjects(addedObjects),
			this.#cache.deleteObjects(deletedIds),
		]);
	}
}
