// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSerializedBcs } from '@mysten/bcs';
import type { BcsType, SerializedBcs } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { Argument } from './data/internal.js';

export function createPure<T>(makePure: (value: SerializedBcs<any, any> | Uint8Array) => T) {
	function pure<T extends PureTypeName>(
		type: T extends PureTypeName ? ValidPureTypeName<T> : T,
		value: ShapeFromPureTypeName<T>,
	): T;

	function pure(
		/**
		 * The pure value, serialized to BCS. If this is a Uint8Array, then the value
		 * is assumed to be raw bytes, and will be used directly.
		 */
		value: SerializedBcs<any, any> | Uint8Array,
	): T;

	function pure(
		typeOrSerializedValue?: PureTypeName | SerializedBcs<any, any> | Uint8Array,
		value?: unknown,
	): T {
		if (typeof typeOrSerializedValue === 'string') {
			return makePure(schemaFromName(typeOrSerializedValue).serialize(value as never));
		}

		if (typeOrSerializedValue instanceof Uint8Array || isSerializedBcs(typeOrSerializedValue)) {
			return makePure(typeOrSerializedValue);
		}

		throw new Error('tx.pure must be called either a bcs type name, or a serialized bcs value');
	}

	pure.u8 = (value: number) => makePure(bcs.U8.serialize(value));
	pure.u16 = (value: number) => makePure(bcs.U16.serialize(value));
	pure.u32 = (value: number) => makePure(bcs.U32.serialize(value));
	pure.u64 = (value: bigint | number | string) => makePure(bcs.U64.serialize(value));
	pure.u128 = (value: bigint | number | string) => makePure(bcs.U128.serialize(value));
	pure.u256 = (value: bigint | number | string) => makePure(bcs.U256.serialize(value));
	pure.bool = (value: boolean) => makePure(bcs.Bool.serialize(value));
	pure.string = (value: string) => makePure(bcs.String.serialize(value));
	pure.address = (value: string) => makePure(bcs.Address.serialize(value));
	pure.id = pure.address;
	pure.vector = <Type extends PureTypeName>(
		type: T extends PureTypeName ? ValidPureTypeName<Type> : Type,
		value: Iterable<ShapeFromPureTypeName<Type>> & { length: number },
	) => {
		return makePure(bcs.vector(schemaFromName(type as BasePureType)).serialize(value as never));
	};
	pure.option = <Type extends PureTypeName>(
		type: T extends PureTypeName ? ValidPureTypeName<Type> : Type,
		value: ShapeFromPureTypeName<Type> | null | undefined,
	) => {
		return makePure(bcs.option(schemaFromName(type)).serialize(value as never));
	};

	return pure;
}

export type BasePureType =
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| 'bool'
	| 'id'
	| 'string'
	| 'address';

export type PureTypeName = BasePureType | `vector<${string}>` | `option<${string}>`;
export type ValidPureTypeName<T extends string> = T extends BasePureType
	? PureTypeName
	: T extends `vector<${infer U}>`
		? ValidPureTypeName<U>
		: T extends `option<${infer U}>`
			? ValidPureTypeName<U>
			: PureTypeValidationError<T>;

type ShapeFromPureTypeName<T extends PureTypeName> = T extends BasePureType
	? Parameters<ReturnType<typeof createPure<Argument>>[T]>[0]
	: T extends `vector<${infer U extends PureTypeName}>`
		? ShapeFromPureTypeName<U>[]
		: T extends `option<${infer U extends PureTypeName}>`
			? ShapeFromPureTypeName<U> | null
			: never;

type PureTypeValidationError<T extends string> = T & {
	error: `Invalid Pure type name: ${T}`;
};

function schemaFromName<T extends PureTypeName>(
	name: T extends PureTypeName ? ValidPureTypeName<T> : T,
): BcsType<ShapeFromPureTypeName<T>> {
	switch (name) {
		case 'u8':
			return bcs.u8() as never;
		case 'u16':
			return bcs.u16() as never;
		case 'u32':
			return bcs.u32() as never;
		case 'u64':
			return bcs.u64() as never;
		case 'u128':
			return bcs.u128() as never;
		case 'u256':
			return bcs.u256() as never;
		case 'bool':
			return bcs.bool() as never;
		case 'string':
			return bcs.string() as never;
		case 'id':
		case 'address':
			return bcs.Address as never;
	}

	const generic = name.match(/^(vector|option)<(.+)>$/);
	if (generic) {
		const [kind, inner] = generic.slice(1);
		if (kind === 'vector') {
			return bcs.vector(schemaFromName(inner as PureTypeName)) as never;
		} else {
			return bcs.option(schemaFromName(inner as PureTypeName)) as never;
		}
	}

	throw new Error(`Invalid Pure type name: ${name}`);
}
