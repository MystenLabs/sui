// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import type { Input } from 'valibot';
import { parse } from 'valibot';

import { bcs } from '../bcs/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type {
	Argument,
	CallArg,
	GasData,
	Transaction,
	TransactionExpiration,
} from './blockData/internal.js';
import { TransactionBlockData } from './blockData/internal.js';
import { transactionBlockDataFromV1 } from './blockData/v1.js';
import type { SerializedTransactionBlockDataV1 } from './blockData/v1.js';
import type { SerializedTransactionBlockDataV2 } from './blockData/v2.js';
import { hashTypedData } from './hash.js';

function prepareSuiAddress(address: string) {
	return normalizeSuiAddress(address).replace('0x', '');
}

export class TransactionBlockDataBuilder implements TransactionBlockData {
	static fromKindBytes(bytes: Uint8Array) {
		const kind = bcs.TransactionKind.parse(bytes);

		const programmableTx = kind.ProgrammableTransaction;
		if (!programmableTx) {
			throw new Error('Unable to deserialize from bytes.');
		}

		return TransactionBlockDataBuilder.restore({
			version: 2,
			sender: null,
			expiration: null,
			gasData: {
				budget: null,
				owner: null,
				payment: null,
				price: null,
			},
			inputs: programmableTx.inputs,
			transactions: programmableTx.transactions,
		});
	}

	static fromBytes(bytes: Uint8Array) {
		const rawData = bcs.TransactionData.parse(bytes);
		const data = rawData?.V1;
		const programmableTx = data.kind.ProgrammableTransaction;

		if (!data || !programmableTx) {
			throw new Error('Unable to deserialize from bytes.');
		}

		return TransactionBlockDataBuilder.restore({
			version: 2,
			sender: data.sender,
			expiration: data.expiration,
			gasData: data.gasData,
			inputs: programmableTx.inputs,
			transactions: programmableTx.transactions,
		});
	}

	static restore(
		data:
			| Input<typeof SerializedTransactionBlockDataV2>
			| Input<typeof SerializedTransactionBlockDataV1>,
	) {
		if (data.version === 2) {
			return new TransactionBlockDataBuilder(parse(TransactionBlockData, data));
		} else {
			return new TransactionBlockDataBuilder(
				parse(TransactionBlockData, transactionBlockDataFromV1(data)),
			);
		}
	}

	/**
	 * Generate transaction digest.
	 *
	 * @param bytes BCS serialized transaction data
	 * @returns transaction digest.
	 */
	static getDigestFromBytes(bytes: Uint8Array) {
		const hash = hashTypedData('TransactionData', bytes);
		return toB58(hash);
	}

	// @deprecated use gasData instead
	get gasConfig() {
		return this.gasData;
	}
	// @deprecated use gasData instead
	set gasConfig(value) {
		this.gasData = value;
	}

	version = 2 as const;
	sender: string | null;
	expiration: TransactionExpiration | null;
	gasData: GasData;
	inputs: CallArg[];
	transactions: Transaction[];

	constructor(clone?: TransactionBlockData) {
		this.sender = clone?.sender ?? null;
		this.expiration = clone?.expiration ?? null;
		this.inputs = clone?.inputs ?? [];
		this.transactions = clone?.transactions ?? [];
		this.gasData = clone?.gasData ?? {
			budget: null,
			price: null,
			owner: null,
			payment: null,
		};
	}

	build({
		maxSizeBytes = Infinity,
		overrides,
		onlyTransactionKind,
	}: {
		maxSizeBytes?: number;
		overrides?: {
			expiration?: TransactionExpiration;
			sender?: string;
			// @deprecated use gasData instead
			gasConfig?: Partial<GasData>;
			gasData?: Partial<GasData>;
		};
		onlyTransactionKind?: boolean;
	} = {}) {
		// TODO validate that inputs and intents are actually resolved
		const inputs = this.inputs as (typeof bcs.CallArg.$inferInput)[];
		const transactions = this.transactions as Extract<
			Transaction<Exclude<Argument, { IntentResult: unknown } | { NestedIntentResult: unknown }>>,
			{ Upgrade: unknown }
		>[];

		const kind = {
			ProgrammableTransaction: {
				inputs,
				transactions,
			},
		};

		if (onlyTransactionKind) {
			return bcs.TransactionKind.serialize(kind, { maxSize: maxSizeBytes }).toBytes();
		}

		const expiration = overrides?.expiration ?? this.expiration;
		const sender = overrides?.sender ?? this.sender;
		const gasData = { ...this.gasData, ...overrides?.gasConfig, ...overrides?.gasData };

		if (!sender) {
			throw new Error('Missing transaction sender');
		}

		if (!gasData.budget) {
			throw new Error('Missing gas budget');
		}

		if (!gasData.payment) {
			throw new Error('Missing gas payment');
		}

		if (!gasData.price) {
			throw new Error('Missing gas price');
		}

		const transactionData = {
			sender: prepareSuiAddress(sender),
			expiration: expiration ? expiration : { None: true },
			gasData: {
				payment: gasData.payment,
				owner: prepareSuiAddress(this.gasData.owner ?? sender),
				price: BigInt(gasData.price),
				budget: BigInt(gasData.budget),
			},
			kind: {
				ProgrammableTransaction: {
					inputs,
					transactions,
				},
			},
		};

		return bcs.TransactionData.serialize(
			{ V1: transactionData },
			{ maxSize: maxSizeBytes },
		).toBytes();
	}

	addInput<T extends 'object' | 'pure'>(type: T, arg: CallArg) {
		const index = this.inputs.length;
		this.inputs.push(arg);
		return { Input: index, type, $kind: 'Input' as const };
	}

	getInputUses(index: number, fn: (arg: Argument, tx: Transaction) => void) {
		this.mapArguments((arg, tx) => {
			if (arg.$kind === 'Input' && arg.Input === index) {
				fn(arg, tx);
			}

			return arg;
		});
	}

	mapArguments(fn: (arg: Argument, tx: Transaction) => Argument) {
		for (const tx of this.transactions) {
			switch (tx.$kind) {
				case 'MoveCall':
					tx.MoveCall.arguments = tx.MoveCall.arguments.map((arg) => fn(arg, tx));
					break;
				case 'TransferObjects':
					tx.TransferObjects.objects = tx.TransferObjects.objects.map((arg) => fn(arg, tx));
					tx.TransferObjects.address = fn(tx.TransferObjects.address, tx);
					break;
				case 'SplitCoins':
					tx.SplitCoins.coin = fn(tx.SplitCoins.coin, tx);
					tx.SplitCoins.amounts = tx.SplitCoins.amounts.map((arg) => fn(arg, tx));
					break;
				case 'MergeCoins':
					tx.MergeCoins.destination = fn(tx.MergeCoins.destination, tx);
					tx.MergeCoins.sources = tx.MergeCoins.sources.map((arg) => fn(arg, tx));
					break;
				case 'MakeMoveVec':
					tx.MakeMoveVec.elements = tx.MakeMoveVec.elements.map((arg) => fn(arg, tx));
					break;
				case 'Upgrade':
					tx.Upgrade.ticket = fn(tx.Upgrade.ticket, tx);
					break;
				case '$Intent':
					const inputs = tx.$Intent.inputs;
					tx.$Intent.inputs = {};

					for (const [key, value] of Object.entries(inputs)) {
						tx.$Intent.inputs[key] = Array.isArray(value)
							? value.map((arg) => fn(arg, tx))
							: fn(value, tx);
					}

					break;
				case 'Publish':
					break;
				default:
					throw new Error(`Unexpected transaction kind: ${(tx as { $kind: unknown }).$kind}`);
			}
		}
	}

	replaceTransaction(index: number, replacement: Transaction | Transaction[]) {
		if (!Array.isArray(replacement)) {
			this.transactions[index] = replacement;
			return;
		}

		const sizeDiff = replacement.length - 1;
		this.transactions.splice(index, 1, ...replacement);

		if (sizeDiff !== 0) {
			this.mapArguments((arg) => {
				switch (arg.$kind) {
					case 'Result':
						if (arg.Result > index) {
							arg.Result += sizeDiff;
						}
						break;

					case 'NestedResult':
						if (arg.NestedResult[0] > index) {
							arg.NestedResult[0] += sizeDiff;
						}
						break;
				}
				return arg;
			});
		}
	}

	getDigest() {
		const bytes = this.build({ onlyTransactionKind: false });
		return TransactionBlockDataBuilder.getDigestFromBytes(bytes);
	}

	snapshot(): TransactionBlockData {
		return parse(TransactionBlockData, this);
	}
}
