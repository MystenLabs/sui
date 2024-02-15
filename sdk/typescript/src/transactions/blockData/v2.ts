// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BaseSchema, Output } from 'valibot';
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
	recursive,
	string,
	transform,
	tuple,
	union,
	unknown,
} from 'valibot';

import type { StructTag as StructTagType, TypeTag as TypeTagType } from '../../bcs/index.js';
import { isValidSuiAddress, normalizeSuiAddress } from '../../utils/sui-types.js';

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

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_object.rs#L661-L668
export const ObjectRef = object({
	digest: string(),
	objectId: SuiAddress,
	version: JsonU64,
});

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1680-L1692
const SuiArgument = union([
	object({ GasCoin: nullable(literal(true)) }),
	object({ Input: number([integer()]) }),
	object({ Result: number([integer()]) }),
	object({ NestedResult: tuple([number([integer()]), number([integer()])]) }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1143-L1153
const GasData = object({
	budget: nullable(JsonU64),
	price: nullable(JsonU64),
	owner: nullable(SuiAddress),
	payment: nullable(array(ObjectRef)),
});

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1719-L1732
const SuiProgrammableMoveCall = object({
	package: ObjectID,
	module: string(),
	function: string(),
	typeArguments: array(string()),
	arguments: array(SuiArgument),
});

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1578-L1601
const SuiTransaction = union([
	object({ MoveCall: SuiProgrammableMoveCall }),
	object({ TransferObjects: tuple([array(SuiArgument), SuiArgument]) }),
	object({ SplitCoins: tuple([SuiArgument, array(SuiArgument)]) }),
	object({ MergeCoins: tuple([SuiArgument, array(SuiArgument)]) }),
	object({ Publish: array(ObjectID) }),
	object({ Upgrade: tuple([array(ObjectID), ObjectID, SuiArgument]) }),
	object({
		MakeMoveVec: tuple([
			union([object({ None: nullable(literal(true)) }), object({ Some: string() })]),
			array(SuiArgument),
		]),
	}),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/external-crates/move/crates/move-core-types/src/language_storage.rs#L33-L59
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

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1995-L2024
const SuiObjectArg = union([
	object({ ImmOrOwnedObject: ObjectRef }),
	object({
		SharedObject: object({ objectId: ObjectID, initialSharedVersion: JsonU64, mutable: boolean() }),
	}),
	object({ Receiving: ObjectRef }),
]);

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1975-L1980
const SuiPureValue = object({
	valueType: union([object({ None: nullable(literal(true)) }), object({ Some: TypeTag })]),
	value: BCSBytes,
});

// https://github.com/MystenLabs/sui/blob/f2601e580e5ec26012669de04fb888ece12bbc06/crates/sui-graphql-rpc/src/types/open_move_type.rs#L86-L105
type OpenMoveTypeSignatureBody =
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

// https://github.com/MystenLabs/sui/blob/f2601e580e5ec26012669de04fb888ece12bbc06/crates/sui-graphql-rpc/src/types/open_move_type.rs#L69-L82
const OpenMoveTypeSignature = object({
	ref: nullable(union([literal('&'), literal('&mut')])),
	body: OpenMoveTypeSignatureBody,
});

// https://github.com/MystenLabs/sui/blob/cea8742e810142a8145fd83c4c142d61e561004a/crates/sui-json-rpc-types/src/sui_transaction.rs#L1912-L1917
const SuiCallArg = union([
	object({ Object: SuiObjectArg }),
	object({ Pure: SuiPureValue }),
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

// https://github.com/MystenLabs/sui/blob/f2601e580e5ec26012669de04fb888ece12bbc06/crates/sui-types/src/transaction.rs#L1395-L1401
const TransactionExpiration = union([
	object({ None: nullable(literal(true)) }),
	object({ Epoch: JsonU64 }),
]);

export const TransactionBlockState = object({
	version: literal(2),
	features: array(string()),
	sender: nullish(SuiAddress),
	expiration: nullish(TransactionExpiration),
	gasData: GasData,
	inputs: array(SuiCallArg),
	transactions: array(SuiTransaction),
});

export type TransactionBlockState = Output<typeof TransactionBlockState>;
