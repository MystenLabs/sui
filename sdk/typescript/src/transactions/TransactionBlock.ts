// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';
import { fromB64, isSerializedBcs } from '@mysten/bcs';
import { is, mask } from 'superstruct';

import type { ProtocolConfig, SuiClient } from '../client/index.js';
import type { SignatureWithBytes, Signer } from '../cryptography/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import { getIdFromCallArg, Inputs, ObjectCallArg, SuiObjectRef } from './Inputs.js';
import { createPure } from './pure.js';
import type { TransactionExpiration } from './TransactionBlockData.js';
import { TransactionBlockDataBuilder } from './TransactionBlockData.js';
import { DefaultTransactionBlockFeatures } from './TransactionBlockPlugin.js';
import type { TransactionArgument, TransactionType } from './Transactions.js';
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

interface BuildOptions {
	client?: SuiClient;
	onlyTransactionKind?: boolean;
	/** Define a protocol config to build against, instead of having it fetched from the provider at build time. */
	protocolConfig?: ProtocolConfig;
	/** Define limits that are used when building the transaction. In general, we recommend using the protocol configuration instead of defining limits. */
	limits?: Limits;
}

interface SignOptions extends BuildOptions {
	signer: Signer;
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

		const inserted = this.#blockData.inputs.find(
			(i) => i.type === 'object' && id === getIdFromCallArg(i.value),
		) as Extract<TransactionArgument, { type?: 'object' }> | undefined;

		// Upgrade shared object inputs to mutable if needed:
		if (
			inserted &&
			is(inserted.value, ObjectCallArg) &&
			'Shared' in inserted.value.Object &&
			is(value, ObjectCallArg) &&
			'Shared' in value.Object
		) {
			inserted.value.Object.Shared.mutable =
				inserted.value.Object.Shared.mutable || value.Object.Shared.mutable;
		}

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

	/**
	 * Prepare the transaction by validating the transaction data and resolving all inputs
	 * so that it can be built into bytes.
	 */
	async #prepare(options: BuildOptions) {
		if (!options.onlyTransactionKind && !this.#blockData.sender) {
			throw new Error('Missing transaction sender');
		}

		if (!options.protocolConfig && !options.limits && options.client) {
			options.protocolConfig = await options.client.getProtocolConfig();
		}

		const plugins = new DefaultTransactionBlockFeatures([], () => expectClient(options));
		await plugins.normalizeInputs(this.#blockData);
		await plugins.resolveObjectReferences(this.#blockData);

		if (!options.onlyTransactionKind) {
			await plugins.setGasPrice(this.#blockData);

			await plugins.setGasBudget(this.#blockData, {
				maxTxGas: this.#getConfig('maxTxGas', options),
				maxTxSizeBytes: this.#getConfig('maxTxSizeBytes', options),
			});

			await plugins.setGasPayment(this.#blockData, {
				maxGasObjects: this.#getConfig('maxGasObjects', options),
			});
		}

		await plugins.validate(this.#blockData, {
			maxPureArgumentSize: this.#getConfig('maxPureArgumentSize', options),
		});
	}
}
