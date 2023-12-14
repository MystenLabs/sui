// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, mask } from 'superstruct';

import { bcs } from '../bcs/index.js';
import type { SuiClient } from '../client/client.js';
import type { SuiMoveNormalizedType } from '../types/normalized.js';
import {
	extractMutableReference,
	extractReference,
	extractStructTag,
} from '../types/normalized.js';
import type { SuiObjectResponse } from '../types/objects.js';
import { getObjectReference, SuiObjectRef } from '../types/objects.js';
import { SUI_TYPE_ARG } from '../utils/index.js';
import { normalizeSuiAddress, normalizeSuiObjectId } from '../utils/sui-types.js';
import { BuilderCallArg, Inputs, isMutableSharedObjectInput, PureCallArg } from './Inputs.js';
import { getPureSerializationType, isTxContext } from './serializer.js';
import type { TransactionBlockDataBuilder } from './TransactionBlockData.js';
import type { MoveCallTransaction, TransactionBlockInput } from './Transactions.js';

export interface TransactionBlockFeatureRequests {
	'txb:normalizeInputs': object;
	'txb:resolveObjectReferences': object;
	'txb:setGasPrice': object;
	'txb:setGasBudget': {
		maxTxGas: number;
		maxTxSizeBytes: number;
	};
	'txb:setGasPayment': {
		maxGasObjects: number;
	};
	'txb:validate': {
		maxPureArgumentSize: number;
	};
}

export type ResolveFeatureFn = <T extends keyof TransactionBlockFeatureRequests>(
	feature: T,
	blockData: TransactionBlockDataBuilder,
	data?: TransactionBlockFeatureRequests[T],
) => Promise<void>;

export interface FeatureProvider {
	resolveFeature: ResolveFeatureFn;
}

// The maximum objects that can be fetched at once using multiGetObjects.
const MAX_OBJECTS_PER_FETCH = 50;

// An amount of gas (in gas units) that is added to transactions as an overhead to ensure transactions do not fail.
const GAS_SAFE_OVERHEAD = 1000n;

const chunk = <T>(arr: T[], size: number): T[][] =>
	Array.from({ length: Math.ceil(arr.length / size) }, (_, i) =>
		arr.slice(i * size, i * size + size),
	);

export class DefaultFeatureProvider implements FeatureProvider {
	#providers: FeatureProvider[];
	#getClient: () => SuiClient;

	constructor(providers: FeatureProvider[], getClient: () => SuiClient) {
		this.#providers = providers;
		this.#getClient = getClient;
	}

	resolveFeature: ResolveFeatureFn = async (feature, blockData, data) => {
		for (const provider of this.#providers) {
			if (provider.resolveFeature) {
				await provider.resolveFeature(feature, blockData, data);
			}
		}

		switch (feature) {
			case 'txb:setGasPrice': {
				await this.#setGasPrice(blockData);
				break;
			}
			case 'txb:normalizeInputs': {
				await this.#normalizeInputs(blockData);
				break;
			}
			case 'txb:resolveObjectReferences': {
				await this.#resolveObjectReferences(blockData);
				break;
			}

			case 'txb:setGasBudget': {
				await this.#setGasBudget(
					blockData,
					data as TransactionBlockFeatureRequests['txb:setGasBudget'],
				);
				break;
			}

			case 'txb:setGasPayment': {
				await this.#setGasPayment(
					blockData,
					data as TransactionBlockFeatureRequests['txb:setGasPayment'],
				);
				break;
			}

			case 'txb:validate': {
				this.#validate(blockData, data as TransactionBlockFeatureRequests['txb:validate']);
				break;
			}
		}
	};

	#setGasPrice = async (blockData: TransactionBlockDataBuilder) => {
		if (blockData.gasConfig.price) {
			return;
		}

		blockData.gasConfig.price = await this.#getClient().getReferenceGasPrice();
	};

	#setGasBudget = async (
		blockData: TransactionBlockDataBuilder,
		options: NonNullable<TransactionBlockFeatureRequests['txb:setGasBudget']>,
	) => {
		if (!blockData.gasConfig.budget) {
			const dryRunResult = await this.#getClient().dryRunTransactionBlock({
				transactionBlock: blockData.build({
					maxSizeBytes: options.maxTxSizeBytes,
					overrides: {
						gasConfig: {
							budget: String(options.maxTxGas),
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

			// Set the budget to max(computation, computation + storage - rebate)
			blockData.gasConfig.budget = String(
				gasBudget > baseComputationCostWithOverhead ? gasBudget : baseComputationCostWithOverhead,
			);
		}
	};

	// The current default is just picking _all_ coins we can which may not be ideal.
	#setGasPayment = async (
		blockData: TransactionBlockDataBuilder,
		{ maxGasObjects }: TransactionBlockFeatureRequests['txb:setGasPayment'],
	) => {
		if (blockData.gasConfig.payment) {
			if (blockData.gasConfig.payment.length > maxGasObjects) {
				throw new Error(`Payment objects exceed maximum amount: ${maxGasObjects}`);
			}
		}

		// Early return if the payment is already set:
		if (blockData.gasConfig.payment) {
			return;
		}

		const gasOwner = blockData.gasConfig.owner ?? blockData.sender;

		const coins = await this.#getClient().getCoins({
			owner: gasOwner!,
			coinType: SUI_TYPE_ARG,
		});

		const paymentCoins = coins.data
			// Filter out coins that are also used as input:
			.filter((coin) => {
				const matchingInput = blockData.inputs.find((input) => {
					if (
						is(input.value, BuilderCallArg) &&
						'Object' in input.value &&
						'ImmOrOwned' in input.value.Object
					) {
						return coin.coinObjectId === input.value.Object.ImmOrOwned.objectId;
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

		blockData.gasConfig.payment = paymentCoins.map((payment) => mask(payment, SuiObjectRef));
	};

	#resolveObjectReferences = async (blockData: TransactionBlockDataBuilder) => {
		const { inputs } = blockData;

		// Keep track of the object references that will need to be resolved at the end of the transaction.
		// We keep the input by-reference to avoid needing to re-resolve it:
		const objectsToResolve: {
			id: string;
			input: TransactionBlockInput;
			normalizedType?: SuiMoveNormalizedType;
		}[] = [];

		inputs.forEach((input) => {
			if (typeof input.value === 'string' && (input.type === 'object' || input.normalizedType)) {
				// The input is a string that we need to resolve to an object reference:
				objectsToResolve.push({
					id: normalizeSuiAddress(input.value),
					input,
					normalizedType: input.normalizedType,
				});
			}
		});

		if (objectsToResolve.length) {
			const dedupedIds = [...new Set(objectsToResolve.map(({ id }) => id))];
			const objectChunks = chunk(dedupedIds, MAX_OBJECTS_PER_FETCH);
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

			let objectsById = new Map(
				dedupedIds.map((id, index) => {
					return [id, objects[index]];
				}),
			);

			const invalidObjects = Array.from(objectsById)
				.filter(([_, obj]) => obj.error)
				.map(([id, _]) => id);
			if (invalidObjects.length) {
				throw new Error(`The following input objects are invalid: ${invalidObjects.join(', ')}`);
			}

			objectsToResolve.forEach(({ id, input, normalizedType }) => {
				const object = objectsById.get(id)!;
				const owner = object.data?.owner;
				const initialSharedVersion =
					owner && typeof owner === 'object' && 'Shared' in owner
						? owner.Shared.initial_shared_version
						: undefined;

				if (initialSharedVersion) {
					// There could be multiple transactions that reference the same shared object.
					// If one of them is a mutable reference or taken by value, then we should mark the input
					// as mutable.
					const isByValue =
						normalizedType != null &&
						extractMutableReference(normalizedType) == null &&
						extractReference(normalizedType) == null;
					const mutable =
						isMutableSharedObjectInput(input.value) ||
						isByValue ||
						(normalizedType != null && extractMutableReference(normalizedType) != null);

					input.value = Inputs.SharedObjectRef({
						objectId: id,
						initialSharedVersion,
						mutable,
					});
				} else if (normalizedType && isReceivingType(normalizedType)) {
					input.value = Inputs.ReceivingRef(getObjectReference(object)!);
				} else {
					input.value = Inputs.ObjectRef(getObjectReference(object as SuiObjectResponse)!);
				}
			});
		}
	};

	#normalizeInputs = async (blockData: TransactionBlockDataBuilder) => {
		const { inputs, transactions } = blockData;
		const moveModulesToResolve: MoveCallTransaction[] = [];

		transactions.forEach((transaction) => {
			// Special case move call:
			if (transaction.kind === 'MoveCall') {
				// Determine if any of the arguments require encoding.
				// - If they don't, then this is good to go.
				// - If they do, then we need to fetch the normalized move module.
				const needsResolution = transaction.arguments.some(
					(arg) => arg.kind === 'Input' && !is(inputs[arg.index].value, BuilderCallArg),
				);

				if (needsResolution) {
					moveModulesToResolve.push(transaction);
				}
			}

			// Special handling for values that where previously encoded using the wellKnownEncoding pattern.
			// This should only happen when transaction block data was hydrated from an old version of the SDK
			if (transaction.kind === 'SplitCoins') {
				transaction.amounts.forEach((amount) => {
					if (amount.kind === 'Input') {
						const input = inputs[amount.index];
						if (typeof input.value !== 'object') {
							input.value = Inputs.Pure(bcs.U64.serialize(input.value));
						}
					}
				});
			}

			if (transaction.kind === 'TransferObjects') {
				if (transaction.address.kind === 'Input') {
					const input = inputs[transaction.address.index];
					if (typeof input.value !== 'object') {
						input.value = Inputs.Pure(bcs.Address.serialize(input.value));
					}
				}
			}
		});

		if (moveModulesToResolve.length) {
			await Promise.all(
				moveModulesToResolve.map(async (moveCall) => {
					const [packageId, moduleName, functionName] = moveCall.target.split('::');

					const normalized = await this.#getClient().getNormalizedMoveFunction({
						package: normalizeSuiObjectId(packageId),
						module: moduleName,
						function: functionName,
					});

					// Entry functions can have a mutable reference to an instance of the TxContext
					// struct defined in the TxContext module as the last parameter. The caller of
					// the function does not need to pass it in as an argument.
					const hasTxContext =
						normalized.parameters.length > 0 && isTxContext(normalized.parameters.at(-1)!);

					const params = hasTxContext
						? normalized.parameters.slice(0, normalized.parameters.length - 1)
						: normalized.parameters;

					if (params.length !== moveCall.arguments.length) {
						throw new Error('Incorrect number of arguments.');
					}

					params.forEach((param, i) => {
						const arg = moveCall.arguments[i];
						if (arg.kind !== 'Input') return;
						const input = inputs[arg.index];
						// Skip if the input is already resolved
						if (is(input.value, BuilderCallArg)) return;

						const inputValue = input.value;

						const serType = getPureSerializationType(param, inputValue);

						if (serType) {
							input.value = Inputs.Pure(inputValue, serType);
							return;
						}

						const structVal = extractStructTag(param);
						if (structVal != null || (typeof param === 'object' && 'TypeParameter' in param)) {
							if (typeof inputValue !== 'string') {
								throw new Error(
									`Expect the argument to be an object id string, got ${JSON.stringify(
										inputValue,
										null,
										2,
									)}`,
								);
							}

							input.normalizedType = param;

							return;
						}

						throw new Error(
							`Unknown call arg type ${JSON.stringify(param, null, 2)} for value ${JSON.stringify(
								inputValue,
								null,
								2,
							)}`,
						);
					});
				}),
			);
		}
	};

	#validate(
		blockData: TransactionBlockDataBuilder,
		{ maxPureArgumentSize }: TransactionBlockFeatureRequests['txb:validate'],
	) {
		// Validate all inputs are the correct size:
		blockData.inputs.forEach((input, index) => {
			if (is(input.value, PureCallArg)) {
				if (input.value.Pure.length > maxPureArgumentSize) {
					throw new Error(
						`Input at index ${index} is too large, max pure input size is ${maxPureArgumentSize} bytes, got ${input.value.Pure.length} bytes`,
					);
				}
			}
		});
	}
}

function isReceivingType(normalizedType: SuiMoveNormalizedType): boolean {
	const tag = extractStructTag(normalizedType);
	if (tag) {
		return (
			tag.Struct.address === '0x2' &&
			tag.Struct.module === 'transfer' &&
			tag.Struct.name === 'Receiving'
		);
	}
	return false;
}
