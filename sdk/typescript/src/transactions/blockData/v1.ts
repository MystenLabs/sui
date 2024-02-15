// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BaseSchema, Output } from 'valibot';
import {
	array,
	bigint,
	custom,
	integer,
	literal,
	nullable,
	nullish,
	number,
	object,
	optional,
	recursive,
	string,
	union,
	unknown,
} from 'valibot';

import type { StructTag as StructTagType, TypeTag as TypeTagType } from '../../bcs/index.js';

export const ObjectRef = object({
	digest: string(),
	objectId: string(),
	version: union([number([integer()]), string(), bigint()]),
});

const TransactionBlockInput = union([
	object({
		kind: literal('Input'),
		index: number([integer()]),
		value: unknown(),
		type: optional(literal('object')),
	}),
	object({
		kind: literal('Input'),
		index: number([integer()]),
		value: unknown(),
		type: literal('pure'),
	}),
]);

const TransactionExpiration = union([
	object({ Epoch: number([integer()]) }),
	object({ None: nullable(literal(true)) }),
]);

const StringEncodedBigint = union(
	[number(), string(), bigint()],
	[
		custom((val) => {
			if (!['string', 'number', 'bigint'].includes(typeof val)) return false;

			try {
				BigInt(val as string);
				return true;
			} catch {
				return false;
			}
		}),
	],
);

export const TypeTag: BaseSchema<TypeTagType> = union([
	object({ bool: nullable(literal(true)) }),
	object({ u8: nullable(literal(true)) }),
	object({ u64: nullable(literal(true)) }),
	object({ u128: nullable(literal(true)) }),
	object({ address: nullable(literal(true)) }),
	object({ signer: nullable(literal(true)) }),
	object({ vector: recursive(() => TypeTag) }),
	object({ struct: recursive(() => StructTag) }),
	object({ u16: nullable(literal(true)) }),
	object({ u32: nullable(literal(true)) }),
	object({ u256: nullable(literal(true)) }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/external-crates/move/crates/move-core-types/src/language_storage.rs#L140-L147
export const StructTag: BaseSchema<StructTagType> = object({
	address: string(),
	module: string(),
	name: string(),
	typeParams: array(TypeTag),
});

const GasConfig = object({
	budget: optional(StringEncodedBigint),
	price: optional(StringEncodedBigint),
	payment: optional(array(ObjectRef)),
	owner: optional(string()),
});

const TransactionArgumentTypes = [
	TransactionBlockInput,
	object({ kind: literal('GasCoin') }),
	object({ kind: literal('Result'), index: number([integer()]) }),
	object({
		kind: literal('NestedResult'),
		index: number([integer()]),
		resultIndex: number([integer()]),
	}),
] as const;

// Generic transaction argument
export const TransactionArgument = union([...TransactionArgumentTypes]);

const MoveCallTransaction = object({
	kind: literal('MoveCall'),
	target: string([
		custom((target) => target.split('::').length === 3),
	]) as BaseSchema<`${string}::${string}::${string}`>,
	typeArguments: array(string()),
	arguments: array(TransactionArgument),
});

const TransferObjectsTransaction = object({
	kind: literal('TransferObjects'),
	objects: array(TransactionArgument),
	address: TransactionArgument,
});

const SplitCoinsTransaction = object({
	kind: literal('SplitCoins'),
	coin: TransactionArgument,
	amounts: array(TransactionArgument),
});

const MergeCoinsTransaction = object({
	kind: literal('MergeCoins'),
	destination: TransactionArgument,
	sources: array(TransactionArgument),
});

const MakeMoveVecTransaction = object({
	kind: literal('MakeMoveVec'),
	type: union([object({ Some: TypeTag }), object({ None: nullable(literal(true)) })]),
	objects: array(TransactionArgument),
});

const PublishTransaction = object({
	kind: literal('Publish'),
	modules: array(array(number([integer()]))),
	dependencies: array(string()),
});

const UpgradeTransaction = object({
	kind: literal('Upgrade'),
	modules: array(array(number([integer()]))),
	dependencies: array(string()),
	packageId: string(),
	ticket: TransactionArgument,
});

const TransactionTypes = [
	MoveCallTransaction,
	TransferObjectsTransaction,
	SplitCoinsTransaction,
	MergeCoinsTransaction,
	PublishTransaction,
	UpgradeTransaction,
	MakeMoveVecTransaction,
] as const;

const TransactionType = union([...TransactionTypes]);

export const SerializedTransactionDataBuilderV1 = object({
	version: literal(1),
	sender: optional(string()),
	expiration: nullish(TransactionExpiration),
	gasConfig: GasConfig,
	inputs: array(TransactionBlockInput),
	transactions: array(TransactionType),
});

export type SerializedTransactionDataBuilderV1 = Output<typeof SerializedTransactionDataBuilderV1>;
