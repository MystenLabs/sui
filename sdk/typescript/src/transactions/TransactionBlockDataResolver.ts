// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { CoinStruct, ProtocolConfig, SuiClient } from '../client/index.js';
import { normalizeSuiAddress, SUI_TYPE_ARG } from '../utils/index.js';
import type { OpenMoveTypeSignature } from './blockData/v2.js';
import { normalizedTypeToMoveTypeSignature } from './serializer.js';
import type { TransactionBlockDataBuilder } from './TransactionBlockData.js';

export type TransactionBlockStep = (
	blockData: TransactionBlockDataBuilder,
	dataResolver: TransactionBlockDataResolver,
) => Promise<void>;

// The maximum objects that can be fetched at once using multiGetObjects.
const MAX_OBJECTS_PER_FETCH = 50;

// An amount of gas (in gas units) that is added to transactions as an overhead to ensure transactions do not fail.
const GAS_SAFE_OVERHEAD = 1000n;

const LIMITS = {
	// The maximum gas that is allowed.
	maxTxGas: 'max_tx_gas',
	// The maximum number of gas objects that can be selected for one transaction.
	maxGasObjects: 'max_gas_payment_objects',
	// The maximum size (in bytes) that the transaction can be:
	maxTxSizeBytes: 'max_tx_size_bytes',
	// The maximum size (in bytes) that pure arguments can be:
	maxPureArgumentSize: 'max_pure_argument_size',
} as const;

type Limits = Partial<Record<keyof typeof LIMITS, number>>;

const DefaultOfflineLimits = {
	maxPureArgumentSize: 16 * 1024,
	maxTxGas: 50_000_000_000,
	maxGasObjects: 256,
	maxTxSizeBytes: 128 * 1024,
} satisfies Limits;

export interface BuildTransactionBlockOptions {
	client?: SuiClient;
	onlyTransactionKind?: boolean;
	/** Define a protocol config to build against, instead of having it fetched from the provider at build time. */
	protocolConfig?: ProtocolConfig;
	/** Define limits that are used when building the transaction. In general, we recommend using the protocol configuration instead of defining limits. */
	limits?: Limits;
	dataResolvers?: TransactionBlockDataResolverPlugin[];
}

export interface SerializeTransactionBlockOptions extends BuildTransactionBlockOptions {
	supportedIntents?: string[];
}

const chunk = <T>(arr: T[], size: number): T[][] =>
	Array.from({ length: Math.ceil(arr.length / size) }, (_, i) =>
		arr.slice(i * size, i * size + size),
	);

export type TransactionBlockDataResolverPlugin = {
	[K in keyof TransactionBlockDataResolver as TransactionBlockDataResolver[K] extends (
		...args: any[]
	) => unknown
		? K
		: never]?: TransactionBlockDataResolver[K] extends (...args: infer Args) => infer R
		? (
				...args: [...args: Args, next: (...args: Args) => R]
		  ) => R extends Promise<infer T> ? T | null | Promise<T | null> : R | null
		: never;
};

export class TransactionBlockDataResolver {
	protected options: SerializeTransactionBlockOptions;
	protected plugins: TransactionBlockDataResolverPlugin[];

	constructor({ dataResolvers = [], ...options }: SerializeTransactionBlockOptions) {
		this.options = options;
		this.plugins = dataResolvers;
	}

	#getClient(): SuiClient {
		if (!this.options.client) {
			throw new Error(
				`No provider passed to Transaction#build, but transaction data was not sufficient to build offline.`,
			);
		}

		return this.options.client;
	}

	async #runPlugins<T extends keyof TransactionBlockDataResolverPlugin>(
		method: T,
		args: Parameters<TransactionBlockDataResolver[T]>,
		cb: () => ReturnType<TransactionBlockDataResolver[T]>,
	): Promise<ReturnType<TransactionBlockDataResolver[T]>> {
		const methods = this.plugins
			.filter((plugin) => method in plugin)
			.map((plugin) => plugin[method]!);

		function run(index: number, runArgs: unknown[]) {
			if (index >= methods.length) {
				return cb();
			}

			const resolver = methods[index];

			return (resolver as (...args: unknown[]) => ReturnType<TransactionBlockDataResolver[T]>)(
				...runArgs,
				(...nextArgs: unknown[]) => run(index + 1, nextArgs),
			);
		}

		return run(0, args);
	}

	#runPluginsSync<T extends keyof TransactionBlockDataResolver>(
		method: T,
		args: Parameters<TransactionBlockDataResolver[T]>,
		cb: () => ReturnType<TransactionBlockDataResolver[T]>,
	): ReturnType<TransactionBlockDataResolver[T]> {
		for (const plugin of this.plugins) {
			if (method in plugin) {
				const result = (
					plugin[method as never] as (
						...args: unknown[]
					) => ReturnType<TransactionBlockDataResolver[T]>
				)(this, ...args);

				if (result != null) {
					return result;
				}
			}
		}

		return cb();
	}

	getLimit(key: keyof typeof LIMITS): number {
		return this.#runPluginsSync('getLimit', [key], () => {
			// Use the limits definition if that exists:
			if (this.options.limits && typeof this.options.limits[key] === 'number') {
				return this.options.limits[key]!;
			}

			if (!this.options.protocolConfig) {
				return DefaultOfflineLimits[key];
			}

			// Fallback to protocol config:
			const attribute = this.options.protocolConfig?.attributes[LIMITS[key]];
			if (!attribute) {
				throw new Error(`Missing expected protocol config: "${LIMITS[key]}"`);
			}

			const value =
				'u64' in attribute
					? attribute.u64
					: 'u32' in attribute
					? attribute.u32
					: 'u16' in attribute
					? attribute.u16
					: attribute.f64;

			if (!value) {
				throw new Error(`Unexpected protocol config value found for: "${LIMITS[key]}"`);
			}

			// NOTE: Technically this is not a safe conversion, but we know all of the values in protocol config are safe
			return Number(value);
		});
	}

	async loadData(blockData: TransactionBlockDataBuilder): Promise<void> {
		await this.#runPlugins('loadData', [blockData], async () => {});
	}

	async getGasPrice(blockData: TransactionBlockDataBuilder): Promise<bigint> {
		return this.#runPlugins('getGasPrice', [blockData], async () => {
			return this.#getClient().getReferenceGasPrice();
		});
	}

	async getGasBudget(blockData: TransactionBlockDataBuilder): Promise<bigint> {
		return this.#runPlugins('getGasBudget', [blockData], async () => {
			const dryRunResult = await this.#getClient().dryRunTransactionBlock({
				transactionBlock: blockData.build({
					maxSizeBytes: this.options.limits?.maxTxSizeBytes,
					overrides: {
						gasData: {
							budget: String(this.getLimit('maxTxGas')),
							payment: [],
						},
					},
				}),
			});

			if (dryRunResult.effects.status.status !== 'success') {
				throw new Error(
					`Dry run failed, could not automatically determine a budget: ${dryRunResult.effects.status.error}`,
					{ cause: dryRunResult },
				);
			}

			const safeOverhead = GAS_SAFE_OVERHEAD * BigInt(blockData.gasConfig.price || 1n);

			const baseComputationCostWithOverhead =
				BigInt(dryRunResult.effects.gasUsed.computationCost) + safeOverhead;

			const gasBudget =
				baseComputationCostWithOverhead +
				BigInt(dryRunResult.effects.gasUsed.storageCost) -
				BigInt(dryRunResult.effects.gasUsed.storageRebate);

			return gasBudget > baseComputationCostWithOverhead
				? gasBudget
				: baseComputationCostWithOverhead;
		});
	}

	async getGasCoins(
		blockData: TransactionBlockDataBuilder,
		owner: string,
	): Promise<
		{
			objectId: string;
			digest: string;
			version: string;
		}[]
	> {
		return this.#runPlugins('getGasCoins', [blockData, owner], async () => {
			const maxGasObjects = this.getLimit('maxGasObjects');

			const coins = await this.#getClient().getCoins({
				owner,
				coinType: SUI_TYPE_ARG,
			});

			const paymentCoins = coins.data
				// Filter out coins that are also used as input:
				.filter((coin) => {
					const matchingInput = blockData.inputs.find((input) => {
						if (input.Object?.ImmOrOwnedObject) {
							return coin.coinObjectId === input.Object.ImmOrOwnedObject.objectId;
						}

						return false;
					});

					return !matchingInput;
				})
				.slice(0, maxGasObjects - 1)
				.map((coin) => ({
					objectId: coin.coinObjectId,
					digest: coin.digest,
					version: coin.version,
				}));

			if (!paymentCoins.length) {
				throw new Error('No valid gas coins found for the transaction.');
			}

			return paymentCoins;
		});
	}

	async getCoins(owner: string, coinType: string): Promise<CoinStruct[]> {
		return this.#runPlugins('getCoins', [owner, coinType], async () => {
			const coins = await this.#getClient().getCoins({
				owner,
				coinType,
			});

			return coins.data;
		});
	}

	async getObjects(ids: string[]): Promise<
		{
			objectId: string;
			digest: string;
			version: string;
			owner: string | null;
			initialSharedVersion: string | null;
		}[]
	> {
		return this.#runPlugins('getObjects', [ids], async () => {
			const objectChunks = chunk(ids, MAX_OBJECTS_PER_FETCH);
			const objects = (
				await Promise.all(
					objectChunks.map((chunk) =>
						this.#getClient().multiGetObjects({
							ids: chunk,
							options: { showOwner: true },
						}),
					),
				)
			).flat();

			const objectsById = new Map(
				ids.map((id, index) => {
					return [id, objects[index]];
				}),
			);

			const invalidObjects = Array.from(objectsById)
				.filter(([_, obj]) => obj.error)
				.map(([id, _]) => id);

			if (invalidObjects.length) {
				throw new Error(`The following input objects are invalid: ${invalidObjects.join(', ')}`);
			}

			return objects.map((object) => {
				if (object.error || !object.data) {
					throw new Error(`Failed to fetch object: ${object.error}`);
				}
				const owner = object.data.owner;
				const initialSharedVersion =
					owner && typeof owner === 'object' && 'Shared' in owner
						? owner.Shared.initial_shared_version
						: null;

				const ownerAddress =
					owner && typeof owner === 'object'
						? 'AddressOwner' in owner
							? owner.AddressOwner
							: 'ObjectOwner' in owner
							? owner.ObjectOwner
							: null
						: null;

				return {
					objectId: object.data.objectId,
					digest: object.data.digest,
					version: object.data.version,
					owner: ownerAddress,
					initialSharedVersion,
				};
			});
		});
	}

	async getMoveFunctionDefinition(ref: {
		package: string;
		module: string;
		function: string;
	}): Promise<{
		package: string;
		module: string;
		function: string;
		parameters: OpenMoveTypeSignature[];
	}> {
		return this.#runPlugins('getMoveFunctionDefinition', [ref], async () => {
			const definition = await this.#getClient().getNormalizedMoveFunction(ref);

			return {
				package: normalizeSuiAddress(ref.package),
				module: ref.module,
				function: ref.function,
				parameters: definition.parameters.map((param) => normalizedTypeToMoveTypeSignature(param)),
			};
		});
	}
}
