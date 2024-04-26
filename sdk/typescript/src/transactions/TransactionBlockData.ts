// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB58 } from '@mysten/bcs';
import type { Infer } from 'superstruct';
import {
	array,
	assert,
	define,
	integer,
	is,
	literal,
	nullable,
	object,
	optional,
	string,
	union,
} from 'superstruct';

import { bcs } from '../bcs/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import { hashTypedData } from './hash.js';
import { BuilderCallArg, PureCallArg, SuiObjectRef } from './Inputs.js';
import { TransactionBlockInput, TransactionType } from './Transactions.js';
import { create } from './utils.js';

export const TransactionExpiration = optional(
	nullable(
		union([object({ Epoch: integer() }), object({ None: union([literal(true), literal(null)]) })]),
	),
);
export type TransactionExpiration = Infer<typeof TransactionExpiration>;

const StringEncodedBigint = define<string | number | bigint>('StringEncodedBigint', (val) => {
	if (!['string', 'number', 'bigint'].includes(typeof val)) return false;

	try {
		BigInt(val as string);
		return true;
	} catch {
		return false;
	}
});

const GasConfig = object({
	budget: optional(StringEncodedBigint),
	price: optional(StringEncodedBigint),
	payment: optional(array(SuiObjectRef)),
	owner: optional(string()),
});
type GasConfig = Infer<typeof GasConfig>;

export const SerializedTransactionDataBuilder = object({
	version: literal(1),
	sender: optional(string()),
	expiration: TransactionExpiration,
	gasConfig: GasConfig,
	inputs: array(TransactionBlockInput),
	transactions: array(TransactionType),
});
export type SerializedTransactionDataBuilder = Infer<typeof SerializedTransactionDataBuilder>;

function prepareSuiAddress(address: string) {
	return normalizeSuiAddress(address).replace('0x', '');
}

export class TransactionBlockDataBuilder {
	static fromKindBytes(bytes: Uint8Array) {
		const kind = bcs.TransactionKind.parse(bytes);
		const programmableTx = 'ProgrammableTransaction' in kind ? kind.ProgrammableTransaction : null;
		if (!programmableTx) {
			throw new Error('Unable to deserialize from bytes.');
		}

		const serialized = create(
			{
				version: 1,
				gasConfig: {},
				inputs: programmableTx.inputs.map((value: unknown, index: number) =>
					create(
						{
							kind: 'Input',
							value,
							index,
							type: is(value, PureCallArg) ? 'pure' : 'object',
						},
						TransactionBlockInput,
					),
				),
				transactions: programmableTx.transactions,
			},
			SerializedTransactionDataBuilder,
		);

		return TransactionBlockDataBuilder.restore(serialized);
	}

	static fromBytes(bytes: Uint8Array) {
		const rawData = bcs.TransactionData.parse(bytes);
		const data = rawData?.V1;
		const programmableTx =
			'ProgrammableTransaction' in data.kind ? data?.kind?.ProgrammableTransaction : null;
		if (!data || !programmableTx) {
			throw new Error('Unable to deserialize from bytes.');
		}

		const serialized = create(
			{
				version: 1,
				sender: data.sender,
				expiration: data.expiration,
				gasConfig: data.gasData,
				inputs: programmableTx.inputs.map((value: unknown, index: number) =>
					create(
						{
							kind: 'Input',
							value,
							index,
							type: is(value, PureCallArg) ? 'pure' : 'object',
						},
						TransactionBlockInput,
					),
				),
				transactions: programmableTx.transactions,
			},
			SerializedTransactionDataBuilder,
		);

		return TransactionBlockDataBuilder.restore(serialized);
	}

	static restore(data: SerializedTransactionDataBuilder) {
		assert(data, SerializedTransactionDataBuilder);
		const transactionData = new TransactionBlockDataBuilder();
		Object.assign(transactionData, data);
		return transactionData;
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

	version = 1 as const;
	sender?: string;
	expiration?: TransactionExpiration;
	gasConfig: GasConfig;
	inputs: TransactionBlockInput[];
	transactions: TransactionType[];

	constructor(clone?: SerializedTransactionDataBuilder) {
		this.sender = clone?.sender;
		this.expiration = clone?.expiration;
		this.gasConfig = clone?.gasConfig ?? {};
		this.inputs = clone?.inputs ?? [];
		this.transactions = clone?.transactions ?? [];
	}

	build({
		maxSizeBytes = Infinity,
		overrides,
		onlyTransactionKind,
	}: {
		maxSizeBytes?: number;
		overrides?: Pick<Partial<TransactionBlockDataBuilder>, 'sender' | 'gasConfig' | 'expiration'>;
		onlyTransactionKind?: boolean;
	} = {}) {
		// Resolve inputs down to values:
		const inputs = this.inputs.map((input) => {
			assert(input.value, BuilderCallArg);
			return input.value;
		});

		const kind = {
			ProgrammableTransaction: {
				inputs,
				transactions: this.transactions,
			},
		};

		if (onlyTransactionKind) {
			return bcs.TransactionKind.serialize(kind, { maxSize: maxSizeBytes }).toBytes();
		}

		const expiration = overrides?.expiration ?? this.expiration;
		const sender = overrides?.sender ?? this.sender;
		const gasConfig = { ...this.gasConfig, ...overrides?.gasConfig };

		if (!sender) {
			throw new Error('Missing transaction sender');
		}

		if (!gasConfig.budget) {
			throw new Error('Missing gas budget');
		}

		if (!gasConfig.payment) {
			throw new Error('Missing gas payment');
		}

		if (!gasConfig.price) {
			throw new Error('Missing gas price');
		}

		const transactionData = {
			sender: prepareSuiAddress(sender),
			expiration: expiration ? expiration : { None: true },
			gasData: {
				payment: gasConfig.payment,
				owner: prepareSuiAddress(this.gasConfig.owner ?? sender),
				price: BigInt(gasConfig.price),
				budget: BigInt(gasConfig.budget),
			},
			kind: {
				ProgrammableTransaction: {
					inputs,
					transactions: this.transactions,
				},
			},
		};

		return bcs.TransactionData.serialize(
			{ V1: transactionData },
			{ maxSize: maxSizeBytes },
		).toBytes();
	}

	getDigest() {
		const bytes = this.build({ onlyTransactionKind: false });
		return TransactionBlockDataBuilder.getDigestFromBytes(bytes);
	}

	snapshot(): SerializedTransactionDataBuilder {
		return create(this, SerializedTransactionDataBuilder);
	}
}
