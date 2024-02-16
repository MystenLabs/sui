// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BaseSchema, Output, UnionOptions, UnionSchema } from 'valibot';
import {
	array,
	boolean,
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
	transform,
	tuple,
	union,
	unknown,
} from 'valibot';

import type { StructTag as StructTagType, TypeTag as TypeTagType } from '../../bcs/index.js';
import { isValidSuiAddress, normalizeSuiAddress } from '../../utils/sui-types.js';

type Merge<T> = T extends object ? { [K in keyof T]: T[K] } : never;
type UnionToEnum<
	T,
	Keys extends string = string & (T extends object ? keyof T : never),
> = T extends object ? Merge<T & { [K in Exclude<Keys, keyof T>]?: never }> : never;

type UnionToEnumSchema<T extends BaseSchema<unknown>> = BaseSchema<UnionToEnum<Output<T>>>;

function enumUnion<T extends UnionOptions>(options: T): UnionToEnumSchema<UnionSchema<T>> {
	return union(options);
}

const SuiAddress = transform(string(), (value) => normalizeSuiAddress(value), [
	custom(isValidSuiAddress),
]);
const ObjectID = SuiAddress;
const BCSBytes = array(number([integer()]));
const JsonU64 = union(
	[string(), number([integer()])],
	[
		custom((val) => {
			try {
				BigInt(val);
				return BigInt(val) >= 0 && BigInt(val) <= 18446744073709551615n;
			} catch {
				return false;
			}
		}, 'Invalid u64'),
	],
);
// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/base_types.rs#L138
// Implemented as a tuple in rust
export const ObjectRef = object({
	digest: string(),
	objectId: SuiAddress,
	version: JsonU64,
});
export type ObjectRef = Output<typeof ObjectRef>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L690-L702
export const Argument = enumUnion([
	object({ GasCoin: literal(true) }),
	object({ Input: number([integer()]), type: optional(literal('pure')) }),
	object({ Input: number([integer()]), type: optional(literal('object')) }),
	object({ Result: number([integer()]) }),
	object({ NestedResult: tuple([number([integer()]), number([integer()])]) }),
]);
export type Argument = Output<typeof Argument>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L1387-L1392
export const GasData = object({
	budget: nullable(JsonU64),
	price: nullable(JsonU64),
	owner: nullable(SuiAddress),
	payment: nullable(array(ObjectRef)),
});
export type GasData = Output<typeof GasData>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/external-crates/move/crates/move-core-types/src/language_storage.rs#L33-L59
export const TypeTag: BaseSchema<TypeTagType> = enumUnion([
	object({ bool: literal(true) }),
	object({ u8: literal(true) }),
	object({ u64: literal(true) }),
	object({ u128: literal(true) }),
	object({ address: literal(true) }),
	object({ signer: literal(true) }),
	object({ vector: recursive(() => TypeTag) }),
	object({ struct: recursive(() => StructTag) }),
	object({ u16: literal(true) }),
	object({ u32: literal(true) }),
	object({ u256: literal(true) }),
]);
export type TypeTag = Output<typeof TypeTag>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/external-crates/move/crates/move-core-types/src/language_storage.rs#L140-L147
export const StructTag: BaseSchema<StructTagType> = object({
	address: string(),
	module: string(),
	name: string(),
	// type_params in rust, should be updated to use camelCase
	typeParams: array(TypeTag),
});
export type StructTag = Output<typeof StructTag>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L707-L718
const ProgrammableMoveCall = object({
	package: ObjectID,
	module: string(),
	function: string(),
	// snake case in rust
	typeArguments: array(TypeTag),
	arguments: array(Argument),
});
export type ProgrammableMoveCall = Output<typeof ProgrammableMoveCall>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L657-L685
const Transaction = enumUnion([
	object({ MoveCall: ProgrammableMoveCall }),
	object({ TransferObjects: tuple([array(Argument), Argument]) }),
	object({ SplitCoins: tuple([Argument, array(Argument)]) }),
	object({ MergeCoins: tuple([Argument, array(Argument)]) }),
	object({ Publish: tuple([array(BCSBytes), array(ObjectID)]) }),
	object({
		MakeMoveVec: tuple([
			enumUnion([object({ None: literal(true) }), object({ Some: TypeTag })]),
			array(Argument),
		]),
	}),
	object({ Upgrade: tuple([array(BCSBytes), array(ObjectID), ObjectID, Argument]) }),
]);
export type Transaction = Output<typeof Transaction>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L102-L114
const ObjectArg = enumUnion([
	object({ ImmOrOwnedObject: ObjectRef }),
	object({
		SharedObject: object({
			objectId: ObjectID,
			// snake case in rust
			initialSharedVersion: JsonU64,
			mutable: boolean(),
		}),
	}),
	object({ Receiving: ObjectRef }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-graphql-rpc/schema/current_progress_schema.graphql#L1614-L1627
export type OpenMoveTypeSignatureBody =
	| 'address'
	| 'bool'
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| { vector: OpenMoveTypeSignatureBody }
	| {
			datatype: {
				package: string;
				module: string;
				type: string;
				typeParameters: OpenMoveTypeSignatureBody[];
			};
	  }
	| { typeParameter: number };

const OpenMoveTypeSignatureBody: BaseSchema<OpenMoveTypeSignatureBody> = union([
	literal('address'),
	literal('bool'),
	literal('u8'),
	literal('u16'),
	literal('u32'),
	literal('u64'),
	literal('u128'),
	literal('u256'),
	object({ vector: recursive(() => OpenMoveTypeSignatureBody) }),
	object({
		datatype: object({
			package: string(),
			module: string(),
			type: string(),
			typeParameters: array(recursive(() => OpenMoveTypeSignatureBody)),
		}),
	}),
	object({ typeParameter: number([integer()]) }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-graphql-rpc/schema/current_progress_schema.graphql#L1609-L1612
const OpenMoveTypeSignature = object({
	ref: nullable(union([literal('&'), literal('&mut')])),
	body: OpenMoveTypeSignatureBody,
});
export type OpenMoveTypeSignature = Output<typeof OpenMoveTypeSignature>;

// https://github.com/MystenLabs/sui/blob/df41d5fa8127634ff4285671a01ead00e519f806/crates/sui-types/src/transaction.rs#L75-L80
const CallArg = enumUnion([
	object({ Object: ObjectArg }),
	object({ Pure: BCSBytes }),
	// added for sui:unresolvedObjectIds
	object({
		UnresolvedObject: object({
			value: string(),
			typeSignature: OpenMoveTypeSignature,
		}),
	}),
	// added for sui:rawValues
	object({
		RawValue: object({
			value: unknown(),
			type: nullish(union([literal('Pure'), literal('Object')])),
		}),
	}),
]);
export type CallArg = Output<typeof CallArg>;

export const NormalizedCallArg = enumUnion([
	object({ Object: ObjectArg }),
	object({ Pure: BCSBytes }),
]);

const TransactionExpiration = enumUnion([
	object({ None: literal(true) }),
	object({ Epoch: JsonU64 }),
]);
export type TransactionExpiration = Output<typeof TransactionExpiration>;

export const TransactionBlockState = object({
	version: literal(2),
	features: array(string()),
	sender: nullish(SuiAddress),
	expiration: nullish(TransactionExpiration),
	gasData: GasData,
	inputs: array(CallArg),
	transactions: array(Transaction),
});
export type TransactionBlockState = Output<typeof TransactionBlockState>;
