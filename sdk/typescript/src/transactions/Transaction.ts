// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SerializedBcs } from '@mysten/bcs';
import { fromBase64, isSerializedBcs } from '@mysten/bcs';
import type { InferInput } from 'valibot';
import { is, parse } from 'valibot';

import type { SuiClient } from '../client/index.js';
import type { SignatureWithBytes, Signer } from '../cryptography/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { TransactionArgument } from './Commands.js';
import { Commands } from './Commands.js';
import type { CallArg, Command } from './data/internal.js';
import { Argument, NormalizedCallArg, ObjectRef, TransactionExpiration } from './data/internal.js';
import { serializeV1TransactionData } from './data/v1.js';
import { SerializedTransactionDataV2 } from './data/v2.js';
import { Inputs } from './Inputs.js';
import type {
	BuildTransactionOptions,
	SerializeTransactionOptions,
	TransactionPlugin,
} from './json-rpc-resolver.js';
import { resolveTransactionData } from './json-rpc-resolver.js';
import { createObjectMethods } from './object.js';
import { createPure } from './pure.js';
import { TransactionDataBuilder } from './TransactionData.js';
import { getIdFromCallArg } from './utils.js';

export type TransactionObjectArgument =
	| Exclude<InferInput<typeof Argument>, { Input: unknown; type?: 'pure' }>
	| ((tx: Transaction) => Exclude<InferInput<typeof Argument>, { Input: unknown; type?: 'pure' }>);

export type TransactionResult = Extract<Argument, { Result: unknown }> &
	Extract<Argument, { NestedResult: unknown }>[];

function createTransactionResult(index: number) {
	const baseResult = { $kind: 'Result' as const, Result: index };

	const nestedResults: {
		$kind: 'NestedResult';
		NestedResult: [number, number];
	}[] = [];
	const nestedResultFor = (
		resultIndex: number,
	): {
		$kind: 'NestedResult';
		NestedResult: [number, number];
	} =>
		(nestedResults[resultIndex] ??= {
			$kind: 'NestedResult' as const,
			NestedResult: [index, resultIndex],
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

const TRANSACTION_BRAND = Symbol.for('@mysten/transaction') as never;

interface SignOptions extends BuildTransactionOptions {
	signer: Signer;
}

export function isTransaction(obj: unknown): obj is Transaction {
	return !!obj && typeof obj === 'object' && (obj as any)[TRANSACTION_BRAND] === true;
}

export type TransactionObjectInput = string | CallArg | TransactionObjectArgument;

interface TransactionPluginRegistry {
	// eslint-disable-next-line @typescript-eslint/ban-types
	buildPlugins: Map<string | Function, TransactionPlugin>;
	// eslint-disable-next-line @typescript-eslint/ban-types
	serializationPlugins: Map<string | Function, TransactionPlugin>;
}

const modulePluginRegistry: TransactionPluginRegistry = {
	buildPlugins: new Map(),
	serializationPlugins: new Map(),
};

const TRANSACTION_REGISTRY_KEY = Symbol.for('@mysten/transaction/registry');
function getGlobalPluginRegistry() {
	try {
		const target = globalThis as {
			[TRANSACTION_REGISTRY_KEY]?: TransactionPluginRegistry;
		};

		if (!target[TRANSACTION_REGISTRY_KEY]) {
			target[TRANSACTION_REGISTRY_KEY] = modulePluginRegistry;
		}

		return target[TRANSACTION_REGISTRY_KEY];
	} catch (e) {
		return modulePluginRegistry;
	}
}

/**
 * Transaction Builder
 */
export class Transaction {
	#serializationPlugins: TransactionPlugin[];
	#buildPlugins: TransactionPlugin[];
	#intentResolvers = new Map<string, TransactionPlugin>();

	/**
	 * Converts from a serialize transaction kind (built with `build({ onlyTransactionKind: true })`) to a `Transaction` class.
	 * Supports either a byte array, or base64-encoded bytes.
	 */
	static fromKind(serialized: string | Uint8Array) {
		const tx = new Transaction();

		tx.#data = TransactionDataBuilder.fromKindBytes(
			typeof serialized === 'string' ? fromBase64(serialized) : serialized,
		);

		return tx;
	}

	/**
	 * Converts from a serialized transaction format to a `Transaction` class.
	 * There are two supported serialized formats:
	 * - A string returned from `Transaction#serialize`. The serialized format must be compatible, or it will throw an error.
	 * - A byte array (or base64-encoded bytes) containing BCS transaction data.
	 */
	static from(transaction: string | Uint8Array | Transaction) {
		const newTransaction = new Transaction();

		if (isTransaction(transaction)) {
			newTransaction.#data = new TransactionDataBuilder(transaction.getData());
		} else if (typeof transaction !== 'string' || !transaction.startsWith('{')) {
			newTransaction.#data = TransactionDataBuilder.fromBytes(
				typeof transaction === 'string' ? fromBase64(transaction) : transaction,
			);
		} else {
			newTransaction.#data = TransactionDataBuilder.restore(JSON.parse(transaction));
		}

		return newTransaction;
	}

	/** @deprecated global plugins should be registered with a name */
	static registerGlobalSerializationPlugin(step: TransactionPlugin): void;
	static registerGlobalSerializationPlugin(name: string, step: TransactionPlugin): void;
	static registerGlobalSerializationPlugin(
		stepOrStep: TransactionPlugin | string,
		step?: TransactionPlugin,
	) {
		getGlobalPluginRegistry().serializationPlugins.set(
			stepOrStep,
			step ?? (stepOrStep as TransactionPlugin),
		);
	}

	static unregisterGlobalSerializationPlugin(name: string) {
		getGlobalPluginRegistry().serializationPlugins.delete(name);
	}

	/** @deprecated global plugins should be registered with a name */
	static registerGlobalBuildPlugin(step: TransactionPlugin): void;
	static registerGlobalBuildPlugin(name: string, step: TransactionPlugin): void;
	static registerGlobalBuildPlugin(
		stepOrStep: TransactionPlugin | string,
		step?: TransactionPlugin,
	) {
		getGlobalPluginRegistry().buildPlugins.set(
			stepOrStep,
			step ?? (stepOrStep as TransactionPlugin),
		);
	}

	static unregisterGlobalBuildPlugin(name: string) {
		getGlobalPluginRegistry().buildPlugins.delete(name);
	}

	addSerializationPlugin(step: TransactionPlugin) {
		this.#serializationPlugins.push(step);
	}

	addBuildPlugin(step: TransactionPlugin) {
		this.#buildPlugins.push(step);
	}

	addIntentResolver(intent: string, resolver: TransactionPlugin) {
		if (this.#intentResolvers.has(intent) && this.#intentResolvers.get(intent) !== resolver) {
			throw new Error(`Intent resolver for ${intent} already exists`);
		}

		this.#intentResolvers.set(intent, resolver);
	}

	setSender(sender: string) {
		this.#data.sender = sender;
	}
	/**
	 * Sets the sender only if it has not already been set.
	 * This is useful for sponsored transaction flows where the sender may not be the same as the signer address.
	 */
	setSenderIfNotSet(sender: string) {
		if (!this.#data.sender) {
			this.#data.sender = sender;
		}
	}
	setExpiration(expiration?: InferInput<typeof TransactionExpiration> | null) {
		this.#data.expiration = expiration ? parse(TransactionExpiration, expiration) : null;
	}
	setGasPrice(price: number | bigint) {
		this.#data.gasConfig.price = String(price);
	}
	setGasBudget(budget: number | bigint) {
		this.#data.gasConfig.budget = String(budget);
	}

	setGasBudgetIfNotSet(budget: number | bigint) {
		if (this.#data.gasData.budget == null) {
			this.#data.gasConfig.budget = String(budget);
		}
	}

	setGasOwner(owner: string) {
		this.#data.gasConfig.owner = owner;
	}
	setGasPayment(payments: ObjectRef[]) {
		this.#data.gasConfig.payment = payments.map((payment) => parse(ObjectRef, payment));
	}

	#data: TransactionDataBuilder;

	/** @deprecated Use `getData()` instead. */
	get blockData() {
		return serializeV1TransactionData(this.#data.snapshot());
	}

	/** Get a snapshot of the transaction data, in JSON form: */
	getData() {
		return this.#data.snapshot();
	}

	// Used to brand transaction classes so that they can be identified, even between multiple copies
	// of the builder.
	get [TRANSACTION_BRAND]() {
		return true;
	}

	// Temporary workaround for the wallet interface accidentally serializing transactions via postMessage
	get pure(): ReturnType<typeof createPure<Argument>> {
		Object.defineProperty(this, 'pure', {
			enumerable: false,
			value: createPure<Argument>((value): Argument => {
				if (isSerializedBcs(value)) {
					return this.#data.addInput('pure', {
						$kind: 'Pure',
						Pure: {
							bytes: value.toBase64(),
						},
					});
				}

				// TODO: we can also do some deduplication here
				return this.#data.addInput(
					'pure',
					is(NormalizedCallArg, value)
						? parse(NormalizedCallArg, value)
						: value instanceof Uint8Array
							? Inputs.Pure(value)
							: { $kind: 'UnresolvedPure', UnresolvedPure: { value } },
				);
			}),
		});

		return this.pure;
	}

	constructor() {
		const globalPlugins = getGlobalPluginRegistry();
		this.#data = new TransactionDataBuilder();
		this.#buildPlugins = [...globalPlugins.buildPlugins.values()];
		this.#serializationPlugins = [...globalPlugins.serializationPlugins.values()];
	}

	/** Returns an argument for the gas coin, to be used in a transaction. */
	get gas() {
		return { $kind: 'GasCoin' as const, GasCoin: true as const };
	}

	/**
	 * Add a new object input to the transaction.
	 */
	object = createObjectMethods(
		(value: TransactionObjectInput): { $kind: 'Input'; Input: number; type?: 'object' } => {
			if (typeof value === 'function') {
				return this.object(value(this));
			}

			if (typeof value === 'object' && is(Argument, value)) {
				return value as { $kind: 'Input'; Input: number; type?: 'object' };
			}

			const id = getIdFromCallArg(value);

			const inserted = this.#data.inputs.find((i) => id === getIdFromCallArg(i));

			// Upgrade shared object inputs to mutable if needed:
			if (
				inserted?.Object?.SharedObject &&
				typeof value === 'object' &&
				value.Object?.SharedObject
			) {
				inserted.Object.SharedObject.mutable =
					inserted.Object.SharedObject.mutable || value.Object.SharedObject.mutable;
			}

			return inserted
				? { $kind: 'Input', Input: this.#data.inputs.indexOf(inserted), type: 'object' }
				: this.#data.addInput(
						'object',
						typeof value === 'string'
							? {
									$kind: 'UnresolvedObject',
									UnresolvedObject: { objectId: normalizeSuiAddress(value) },
								}
							: value,
					);
		},
	);

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

	/** Add a transaction to the transaction */
	add<T = TransactionResult>(command: Command | ((tx: Transaction) => T)): T {
		if (typeof command === 'function') {
			return command(this);
		}

		const index = this.#data.commands.push(command);

		return createTransactionResult(index - 1) as T;
	}

	#normalizeTransactionArgument(arg: TransactionArgument | SerializedBcs<any>) {
		if (isSerializedBcs(arg)) {
			return this.pure(arg);
		}

		return this.#resolveArgument(arg as TransactionArgument);
	}

	#resolveArgument(arg: TransactionArgument): Argument {
		if (typeof arg === 'function') {
			return parse(Argument, arg(this));
		}

		return parse(Argument, arg);
	}

	// Method shorthands:

	splitCoins(
		coin: TransactionObjectArgument | string,
		amounts: (TransactionArgument | SerializedBcs<any> | number | string | bigint)[],
	) {
		return this.add(
			Commands.SplitCoins(
				typeof coin === 'string' ? this.object(coin) : this.#resolveArgument(coin),
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
			Commands.MergeCoins(
				this.object(destination),
				sources.map((src) => this.object(src)),
			),
		);
	}
	publish({ modules, dependencies }: { modules: number[][] | string[]; dependencies: string[] }) {
		return this.add(
			Commands.Publish({
				modules,
				dependencies,
			}),
		);
	}
	upgrade({
		modules,
		dependencies,
		package: packageId,
		ticket,
	}: {
		modules: number[][] | string[];
		dependencies: string[];
		package: string;
		ticket: TransactionObjectArgument | string;
	}) {
		return this.add(
			Commands.Upgrade({
				modules,
				dependencies,
				package: packageId,
				ticket: this.object(ticket),
			}),
		);
	}
	moveCall({
		arguments: args,
		...input
	}:
		| {
				package: string;
				module: string;
				function: string;
				arguments?: (TransactionArgument | SerializedBcs<any>)[];
				typeArguments?: string[];
		  }
		| {
				target: string;
				arguments?: (TransactionArgument | SerializedBcs<any>)[];
				typeArguments?: string[];
		  }) {
		return this.add(
			Commands.MoveCall({
				...input,
				arguments: args?.map((arg) => this.#normalizeTransactionArgument(arg)),
			} as Parameters<typeof Commands.MoveCall>[0]),
		);
	}
	transferObjects(
		objects: (TransactionObjectArgument | string)[],
		address: TransactionArgument | SerializedBcs<any> | string,
	) {
		return this.add(
			Commands.TransferObjects(
				objects.map((obj) => this.object(obj)),
				typeof address === 'string'
					? this.pure.address(address)
					: this.#normalizeTransactionArgument(address),
			),
		);
	}
	makeMoveVec({
		type,
		elements,
	}: {
		elements: (TransactionObjectArgument | string)[];
		type?: string;
	}) {
		return this.add(
			Commands.MakeMoveVec({
				type,
				elements: elements.map((obj) => this.object(obj)),
			}),
		);
	}

	/**
	 * @deprecated Use toJSON instead.
	 * For synchronous serialization, you can use `getData()`
	 * */
	serialize() {
		return JSON.stringify(serializeV1TransactionData(this.#data.snapshot()));
	}

	async toJSON(options: SerializeTransactionOptions = {}): Promise<string> {
		await this.prepareForSerialization(options);
		return JSON.stringify(
			parse(SerializedTransactionDataV2, this.#data.snapshot()),
			(_key, value) => (typeof value === 'bigint' ? value.toString() : value),
			2,
		);
	}

	/** Build the transaction to BCS bytes, and sign it with the provided keypair. */
	async sign(options: SignOptions): Promise<SignatureWithBytes> {
		const { signer, ...buildOptions } = options;
		const bytes = await this.build(buildOptions);
		return signer.signTransaction(bytes);
	}

	/** Build the transaction to BCS bytes. */
	async build(options: BuildTransactionOptions = {}): Promise<Uint8Array> {
		await this.prepareForSerialization(options);
		await this.#prepareBuild(options);
		return this.#data.build({
			onlyTransactionKind: options.onlyTransactionKind,
		});
	}

	/** Derive transaction digest */
	async getDigest(
		options: {
			client?: SuiClient;
		} = {},
	): Promise<string> {
		await this.#prepareBuild(options);
		return this.#data.getDigest();
	}

	/**
	 * Prepare the transaction by validating the transaction data and resolving all inputs
	 * so that it can be built into bytes.
	 */
	async #prepareBuild(options: BuildTransactionOptions) {
		if (!options.onlyTransactionKind && !this.#data.sender) {
			throw new Error('Missing transaction sender');
		}

		await this.#runPlugins([...this.#buildPlugins, resolveTransactionData], options);
	}

	async #runPlugins(plugins: TransactionPlugin[], options: SerializeTransactionOptions) {
		const createNext = (i: number) => {
			if (i >= plugins.length) {
				return () => {};
			}
			const plugin = plugins[i];

			return async () => {
				const next = createNext(i + 1);
				let calledNext = false;
				let nextResolved = false;

				await plugin(this.#data, options, async () => {
					if (calledNext) {
						throw new Error(`next() was call multiple times in TransactionPlugin ${i}`);
					}

					calledNext = true;

					await next();

					nextResolved = true;
				});

				if (!calledNext) {
					throw new Error(`next() was not called in TransactionPlugin ${i}`);
				}

				if (!nextResolved) {
					throw new Error(`next() was not awaited in TransactionPlugin ${i}`);
				}
			};
		};

		await createNext(0)();
	}

	async prepareForSerialization(options: SerializeTransactionOptions) {
		const intents = new Set<string>();
		for (const command of this.#data.commands) {
			if (command.$Intent) {
				intents.add(command.$Intent.name);
			}
		}

		const steps = [...this.#serializationPlugins];

		for (const intent of intents) {
			if (options.supportedIntents?.includes(intent)) {
				continue;
			}

			if (!this.#intentResolvers.has(intent)) {
				throw new Error(`Missing intent resolver for ${intent}`);
			}

			steps.push(this.#intentResolvers.get(intent)!);
		}

		await this.#runPlugins(steps, options);
	}
}
