// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';
import { fromB64, isSerializedBcs } from '@mysten/bcs';
import { is, mask } from 'superstruct';

import { bcs } from '../bcs/index.js';
import type { ProtocolConfig, SuiClient, SuiMoveNormalizedType } from '../client/index.js';
import type { Keypair, SignatureWithBytes } from '../cryptography/index.js';
import type { SuiObjectResponse } from '../types/index.js';
import {
	extractMutableReference,
	extractReference,
	extractStructTag,
	getObjectReference,
	SuiObjectRef,
} from '../types/index.js';
import { SUI_TYPE_ARG } from '../utils/index.js';
import { normalizeSuiAddress, normalizeSuiObjectId } from '../utils/sui-types.js';
import type { ObjectCallArg } from './Inputs.js';
import {
	BuilderCallArg,
	getIdFromCallArg,
	Inputs,
	isMutableSharedObjectInput,
	PureCallArg,
} from './Inputs.js';
import { createPure } from './pure.js';
import { getPureSerializationType, isTxContext } from './serializer.js';
import type { TransactionExpiration } from './TransactionBlockData.js';
import { TransactionBlockDataBuilder } from './TransactionBlockData.js';
import type { MoveCallTransaction, TransactionArgument, TransactionType } from './Transactions.js';
import { TransactionBlockInput, Transactions } from './Transactions.js';
import { create } from './utils.js';

export type TransactionObjectArgument = Exclude<
	TransactionArgument,
	{ kind: 'Input'; type: 'pure' }
>;

export type TransactionResult = Extract<TransactionArgument, { kind: 'Result' }> &
	Extract<TransactionArgument, { kind: 'NestedResult' }>[];

const DefaultOfflineLimits = {
	maxPureArgumentSize: 16 * 1024,
	maxTxGas: 50_000_000_000,
	maxGasObjects: 256,
	maxTxSizeBytes: 128 * 1024,
} satisfies Limits;

function createTransactionResult(index: number): TransactionResult {
	const baseResult: TransactionArgument = { kind: 'Result', index };

	const nestedResults: TransactionArgument[] = [];
	const nestedResultFor = (resultIndex: number): TransactionArgument =>
		(nestedResults[resultIndex] ??= {
			kind: 'NestedResult',
			index,
			resultIndex,
		});

	return new Proxy(baseResult, {
		set() {
			throw new Error(
				'The transaction result is a proxy, and does not support setting properties directly',
			);
		},
		// TODO: Instead of making this return a concrete argument, we should ideally
		// make it reference-based (so that this gets resolved at build-time), which
		// allows re-ordering transactions.
		get(target, property) {
			// This allows this transaction argument to be used in the singular form:
			if (property in target) {
				return Reflect.get(target, property);
			}

			// Support destructuring:
			if (property === Symbol.iterator) {
				return function* () {
					let i = 0;
					while (true) {
						yield nestedResultFor(i);
						i++;
					}
				};
			}

			if (typeof property === 'symbol') return;

			const resultIndex = parseInt(property, 10);
			if (Number.isNaN(resultIndex) || resultIndex < 0) return;
			return nestedResultFor(resultIndex);
		},
	}) as TransactionResult;
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

function expectClient(options: BuildOptions): SuiClient {
	if (!options.client) {
		throw new Error(
			`No provider passed to Transaction#build, but transaction data was not sufficient to build offline.`,
		);
	}

	return options.client;
}

const TRANSACTION_BRAND = Symbol.for('@mysten/transaction');

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

// An amount of gas (in gas units) that is added to transactions as an overhead to ensure transactions do not fail.
const GAS_SAFE_OVERHEAD = 1000n;

// The maximum objects that can be fetched at once using multiGetObjects.
const MAX_OBJECTS_PER_FETCH = 50;

const chunk = <T>(arr: T[], size: number): T[][] =>
	Array.from({ length: Math.ceil(arr.length / size) }, (_, i) =>
		arr.slice(i * size, i * size + size),
	);

interface BuildOptions {
	client?: SuiClient;
	onlyTransactionKind?: boolean;
	/** Define a protocol config to build against, instead of having it fetched from the provider at build time. */
	protocolConfig?: ProtocolConfig;
	/** Define limits that are used when building the transaction. In general, we recommend using the protocol configuration instead of defining limits. */
	limits?: Limits;
}

interface SignOptions extends BuildOptions {
	signer: Keypair;
}

export function isTransactionBlock(obj: unknown): obj is TransactionBlock {
	return !!obj && typeof obj === 'object' && (obj as any)[TRANSACTION_BRAND] === true;
}

export type TransactionObjectInput = string | ObjectCallArg | TransactionObjectArgument;

/**
 * Transaction Builder
 */
export class TransactionBlock {
	/**
	 * Converts from a serialize transaction kind (built with `build({ onlyTransactionKind: true })`) to a `Transaction` class.
	 * Supports either a byte array, or base64-encoded bytes.
	 */
	static fromKind(serialized: string | Uint8Array) {
		const tx = new TransactionBlock();

		tx.#blockData = TransactionBlockDataBuilder.fromKindBytes(
			typeof serialized === 'string' ? fromB64(serialized) : serialized,
		);

		return tx;
	}

	/**
	 * Converts from a serialized transaction format to a `Transaction` class.
	 * There are two supported serialized formats:
	 * - A string returned from `Transaction#serialize`. The serialized format must be compatible, or it will throw an error.
	 * - A byte array (or base64-encoded bytes) containing BCS transaction data.
	 */
	static from(serialized: string | Uint8Array) {
		const tx = new TransactionBlock();

		// Check for bytes:
		if (typeof serialized !== 'string' || !serialized.startsWith('{')) {
			tx.#blockData = TransactionBlockDataBuilder.fromBytes(
				typeof serialized === 'string' ? fromB64(serialized) : serialized,
			);
		} else {
			tx.#blockData = TransactionBlockDataBuilder.restore(JSON.parse(serialized));
		}

		return tx;
	}

	setSender(sender: string) {
		this.#blockData.sender = sender;
	}
	/**
	 * Sets the sender only if it has not already been set.
	 * This is useful for sponsored transaction flows where the sender may not be the same as the signer address.
	 */
	setSenderIfNotSet(sender: string) {
		if (!this.#blockData.sender) {
			this.#blockData.sender = sender;
		}
	}
	setExpiration(expiration?: TransactionExpiration) {
		this.#blockData.expiration = expiration;
	}
	setGasPrice(price: number | bigint) {
		this.#blockData.gasConfig.price = String(price);
	}
	setGasBudget(budget: number | bigint) {
		this.#blockData.gasConfig.budget = String(budget);
	}
	setGasOwner(owner: string) {
		this.#blockData.gasConfig.owner = owner;
	}
	setGasPayment(payments: SuiObjectRef[]) {
		this.#blockData.gasConfig.payment = payments.map((payment) => mask(payment, SuiObjectRef));
	}

	#blockData: TransactionBlockDataBuilder;
	/** Get a snapshot of the transaction data, in JSON form: */
	get blockData() {
		return this.#blockData.snapshot();
	}

	// Used to brand transaction classes so that they can be identified, even between multiple copies
	// of the builder.
	get [TRANSACTION_BRAND]() {
		return true;
	}

	// Temporary workaround for the wallet interface accidentally serializing transaction blocks via postMessage
	get pure(): ReturnType<typeof createPure> {
		Object.defineProperty(this, 'pure', {
			enumerable: false,
			value: createPure((value, type) => {
				if (isSerializedBcs(value)) {
					return this.#input('pure', {
						Pure: Array.from(value.toBytes()),
					});
				}

				// TODO: we can also do some deduplication here
				return this.#input(
					'pure',
					value instanceof Uint8Array
						? Inputs.Pure(value)
						: type
						? Inputs.Pure(value, type)
						: value,
				);
			}),
		});

		return this.pure;
	}

	constructor(transaction?: TransactionBlock) {
		this.#blockData = new TransactionBlockDataBuilder(
			transaction ? transaction.blockData : undefined,
		);
	}

	/** Returns an argument for the gas coin, to be used in a transaction. */
	get gas(): TransactionObjectArgument {
		return { kind: 'GasCoin' };
	}

	/**
	 * Dynamically create a new input, which is separate from the `input`. This is important
	 * for generated clients to be able to define unique inputs that are non-overlapping with the
	 * defined inputs.
	 *
	 * For `Uint8Array` type automatically convert the input into a `Pure` CallArg, since this
	 * is the format required for custom serialization.
	 *
	 */
	#input<T extends 'object' | 'pure'>(type: T, value?: unknown) {
		const index = this.#blockData.inputs.length;
		const input = create(
			{
				kind: 'Input',
				// bigints can't be serialized to JSON, so just string-convert them here:
				value: typeof value === 'bigint' ? String(value) : value,
				index,
				type,
			},
			TransactionBlockInput,
		);
		this.#blockData.inputs.push(input);
		return input as Extract<typeof input, { type: T }>;
	}

	/**
	 * Add a new object input to the transaction.
	 */
	object(value: TransactionObjectInput) {
		if (typeof value === 'object' && 'kind' in value) {
			return value;
		}

		const id = getIdFromCallArg(value);
		// deduplicate
		const inserted = this.#blockData.inputs.find(
			(i) => i.type === 'object' && id === getIdFromCallArg(i.value),
		) as Extract<TransactionArgument, { type?: 'object' }> | undefined;
		return (
			inserted ??
			this.#input('object', typeof value === 'string' ? normalizeSuiAddress(value) : value)
		);
	}

	/**
	 * Add a new object input to the transaction using the fully-resolved object reference.
	 * If you only have an object ID, use `builder.object(id)` instead.
	 */
	objectRef(...args: Parameters<(typeof Inputs)['ObjectRef']>) {
		return this.object(Inputs.ObjectRef(...args));
	}

	/**
	 * Add a new receiving input to the transaction using the fully-resolved object reference.
	 * If you only have an object ID, use `builder.object(id)` instead.
	 */
	receivingRef(...args: Parameters<(typeof Inputs)['ReceivingRef']>) {
		return this.object(Inputs.ReceivingRef(...args));
	}

	/**
	 * Add a new shared object input to the transaction using the fully-resolved shared object reference.
	 * If you only have an object ID, use `builder.object(id)` instead.
	 */
	sharedObjectRef(...args: Parameters<(typeof Inputs)['SharedObjectRef']>) {
		return this.object(Inputs.SharedObjectRef(...args));
	}

	/** Add a transaction to the transaction block. */
	add(transaction: TransactionType) {
		const index = this.#blockData.transactions.push(transaction);
		return createTransactionResult(index - 1);
	}

	#normalizeTransactionArgument(
		arg: TransactionArgument | SerializedBcs<any>,
	): TransactionArgument {
		if (isSerializedBcs(arg)) {
			return this.pure(arg);
		}

		return arg as TransactionArgument;
	}

	// Method shorthands:

	splitCoins(
		coin: TransactionObjectArgument | string,
		amounts: (TransactionArgument | SerializedBcs<any> | number | string | bigint)[],
	) {
		return this.add(
			Transactions.SplitCoins(
				typeof coin === 'string' ? this.object(coin) : coin,
				amounts.map((amount) =>
					typeof amount === 'number' || typeof amount === 'bigint' || typeof amount === 'string'
						? this.pure.u64(amount)
						: this.#normalizeTransactionArgument(amount),
				),
			),
		);
	}
	mergeCoins(
		destination: TransactionObjectArgument | string,
		sources: (TransactionObjectArgument | string)[],
	) {
		return this.add(
			Transactions.MergeCoins(
				typeof destination === 'string' ? this.object(destination) : destination,
				sources.map((src) => (typeof src === 'string' ? this.object(src) : src)),
			),
		);
	}
	publish({ modules, dependencies }: { modules: number[][] | string[]; dependencies: string[] }) {
		return this.add(
			Transactions.Publish({
				modules,
				dependencies,
			}),
		);
	}
	upgrade({
		modules,
		dependencies,
		packageId,
		ticket,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
		packageId: string;
		ticket: TransactionObjectArgument | string;
	}) {
		return this.add(
			Transactions.Upgrade({
				modules,
				dependencies,
				packageId,
				ticket: typeof ticket === 'string' ? this.object(ticket) : ticket,
			}),
		);
	}
	moveCall({
		arguments: args,
		typeArguments,
		target,
	}: {
		arguments?: (TransactionArgument | SerializedBcs<any>)[];
		typeArguments?: string[];
		target: `${string}::${string}::${string}`;
	}) {
		return this.add(
			Transactions.MoveCall({
				arguments: args?.map((arg) => this.#normalizeTransactionArgument(arg)),
				typeArguments,
				target,
			}),
		);
	}
	transferObjects(
		objects: (TransactionObjectArgument | string)[],
		address: TransactionArgument | SerializedBcs<any> | string,
	) {
		return this.add(
			Transactions.TransferObjects(
				objects.map((obj) => (typeof obj === 'string' ? this.object(obj) : obj)),
				typeof address === 'string'
					? this.pure.address(address)
					: this.#normalizeTransactionArgument(address),
			),
		);
	}
	makeMoveVec({
		type,
		objects,
	}: {
		objects: (TransactionObjectArgument | string)[];
		type?: string;
	}) {
		return this.add(
			Transactions.MakeMoveVec({
				type,
				objects: objects.map((obj) => (typeof obj === 'string' ? this.object(obj) : obj)),
			}),
		);
	}

	/**
	 * Serialize the transaction to a string so that it can be sent to a separate context.
	 * This is different from `build` in that it does not serialize to BCS bytes, and instead
	 * uses a separate format that is unique to the transaction builder. This allows
	 * us to serialize partially-complete transactions, that can then be completed and
	 * built in a separate context.
	 *
	 * For example, a dapp can construct a transaction, but not provide gas objects
	 * or a gas budget. The transaction then can be sent to the wallet, where this
	 * information is automatically filled in (e.g. by querying for coin objects
	 * and performing a dry run).
	 */
	serialize() {
		return JSON.stringify(this.#blockData.snapshot());
	}

	#getConfig(key: keyof typeof LIMITS, { protocolConfig, limits }: BuildOptions) {
		// Use the limits definition if that exists:
		if (limits && typeof limits[key] === 'number') {
			return limits[key]!;
		}

		if (!protocolConfig) {
			return DefaultOfflineLimits[key];
		}

		// Fallback to protocol config:
		const attribute = protocolConfig?.attributes[LIMITS[key]];
		if (!attribute) {
			throw new Error(`Missing expected protocol config: "${LIMITS[key]}"`);
		}

		const value =
			'u64' in attribute ? attribute.u64 : 'u32' in attribute ? attribute.u32 : attribute.f64;

		if (!value) {
			throw new Error(`Unexpected protocol config value found for: "${LIMITS[key]}"`);
		}

		// NOTE: Technically this is not a safe conversion, but we know all of the values in protocol config are safe
		return Number(value);
	}

	/** Build the transaction to BCS bytes, and sign it with the provided keypair. */
	async sign(options: SignOptions): Promise<SignatureWithBytes> {
		const { signer, ...buildOptions } = options;
		const bytes = await this.build(buildOptions);
		return signer.signTransactionBlock(bytes);
	}

	/** Build the transaction to BCS bytes. */
	async build(options: BuildOptions = {}): Promise<Uint8Array> {
		await this.#prepare(options);
		return this.#blockData.build({
			maxSizeBytes: this.#getConfig('maxTxSizeBytes', options),
			onlyTransactionKind: options.onlyTransactionKind,
		});
	}

	/** Derive transaction digest */
	async getDigest(
		options: {
			client?: SuiClient;
		} = {},
	): Promise<string> {
		await this.#prepare(options);
		return this.#blockData.getDigest();
	}

	#validate(options: BuildOptions) {
		const maxPureArgumentSize = this.#getConfig('maxPureArgumentSize', options);
		// Validate all inputs are the correct size:
		this.#blockData.inputs.forEach((input, index) => {
			if (is(input.value, PureCallArg)) {
				if (input.value.Pure.length > maxPureArgumentSize) {
					throw new Error(
						`Input at index ${index} is too large, max pure input size is ${maxPureArgumentSize} bytes, got ${input.value.Pure.length} bytes`,
					);
				}
			}
		});
	}

	// The current default is just picking _all_ coins we can which may not be ideal.
	async #prepareGasPayment(options: BuildOptions) {
		if (this.#blockData.gasConfig.payment) {
			const maxGasObjects = this.#getConfig('maxGasObjects', options);
			if (this.#blockData.gasConfig.payment.length > maxGasObjects) {
				throw new Error(`Payment objects exceed maximum amount: ${maxGasObjects}`);
			}
		}

		// Early return if the payment is already set:
		if (options.onlyTransactionKind || this.#blockData.gasConfig.payment) {
			return;
		}

		const gasOwner = this.#blockData.gasConfig.owner ?? this.#blockData.sender;

		const coins = await expectClient(options).getCoins({
			owner: gasOwner!,
			coinType: SUI_TYPE_ARG,
		});

		const paymentCoins = coins.data
			// Filter out coins that are also used as input:
			.filter((coin) => {
				const matchingInput = this.#blockData.inputs.find((input) => {
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
			.slice(0, this.#getConfig('maxGasObjects', options) - 1)
			.map((coin) => ({
				objectId: coin.coinObjectId,
				digest: coin.digest,
				version: coin.version,
			}));

		if (!paymentCoins.length) {
			throw new Error('No valid gas coins found for the transaction.');
		}

		this.setGasPayment(paymentCoins);
	}

	async #prepareGasPrice(options: BuildOptions) {
		if (options.onlyTransactionKind || this.#blockData.gasConfig.price) {
			return;
		}

		this.setGasPrice(await expectClient(options).getReferenceGasPrice());
	}

	async #prepareTransactions(options: BuildOptions) {
		const { inputs, transactions } = this.#blockData;

		const moveModulesToResolve: MoveCallTransaction[] = [];

		// Keep track of the object references that will need to be resolved at the end of the transaction.
		// We keep the input by-reference to avoid needing to re-resolve it:
		const objectsToResolve: {
			id: string;
			input: TransactionBlockInput;
			normalizedType?: SuiMoveNormalizedType;
		}[] = [];

		inputs.forEach((input) => {
			if (input.type === 'object' && typeof input.value === 'string') {
				// The input is a string that we need to resolve to an object reference:
				objectsToResolve.push({ id: normalizeSuiAddress(input.value), input });
				return;
			}
		});

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

					const normalized = await expectClient(options).getNormalizedMoveFunction({
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
							objectsToResolve.push({
								id: inputValue,
								input,
								normalizedType: param,
							});
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

		if (objectsToResolve.length) {
			const dedupedIds = [...new Set(objectsToResolve.map(({ id }) => id))];
			const objectChunks = chunk(dedupedIds, MAX_OBJECTS_PER_FETCH);
			const objects = (
				await Promise.all(
					objectChunks.map((chunk) =>
						expectClient(options).multiGetObjects({
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
	}

	/**
	 * Prepare the transaction by valdiating the transaction data and resolving all inputs
	 * so that it can be built into bytes.
	 */
	async #prepare(options: BuildOptions) {
		if (!options.onlyTransactionKind && !this.#blockData.sender) {
			throw new Error('Missing transaction sender');
		}

		if (!options.protocolConfig && !options.limits && options.client) {
			options.protocolConfig = await options.client.getProtocolConfig();
		}

		await Promise.all([this.#prepareGasPrice(options), this.#prepareTransactions(options)]);

		if (!options.onlyTransactionKind) {
			await this.#prepareGasPayment(options);

			if (!this.#blockData.gasConfig.budget) {
				const dryRunResult = await expectClient(options).dryRunTransactionBlock({
					transactionBlock: this.#blockData.build({
						maxSizeBytes: this.#getConfig('maxTxSizeBytes', options),
						overrides: {
							gasConfig: {
								budget: String(this.#getConfig('maxTxGas', options)),
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

				const safeOverhead = GAS_SAFE_OVERHEAD * BigInt(this.blockData.gasConfig.price || 1n);

				const baseComputationCostWithOverhead =
					BigInt(dryRunResult.effects.gasUsed.computationCost) + safeOverhead;

				const gasBudget =
					baseComputationCostWithOverhead +
					BigInt(dryRunResult.effects.gasUsed.storageCost) -
					BigInt(dryRunResult.effects.gasUsed.storageRebate);

				// Set the budget to max(computation, computation + storage - rebate)
				this.setGasBudget(
					gasBudget > baseComputationCostWithOverhead ? gasBudget : baseComputationCostWithOverhead,
				);
			}
		}

		// Perform final validation on the transaction:
		this.#validate(options);
	}
}
