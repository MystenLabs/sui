// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	MoveStruct,
	MoveValue,
	SuiMoveAbility,
	SuiMoveNormalizedFunction,
	SuiMoveNormalizedModule,
	SuiMoveNormalizedStruct,
	SuiMoveNormalizedType,
} from '@mysten/sui/client';
import { normalizeSuiAddress, parseStructTag } from '@mysten/sui/utils';

import type {
	Rpc_Move_Function_FieldsFragment,
	Rpc_Move_Module_FieldsFragment,
	Rpc_Move_Struct_FieldsFragment,
} from '../generated/queries.js';
import { toShortTypeString } from './util.js';

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
				typeParameters?: [OpenMoveTypeSignatureBody];
			};
	  }
	| { typeParameter: number };

export function mapOpenMoveType(type: { ref?: '&' | '&mut'; body: OpenMoveTypeSignatureBody }) {
	const body = mapNormalizedType(type.body);

	if (type.ref === '&') {
		return {
			Reference: body,
		};
	}

	if (type.ref === '&mut') {
		return {
			MutableReference: body,
		};
	}

	return body;
}

export function mapNormalizedType(type: OpenMoveTypeSignatureBody): SuiMoveNormalizedType {
	switch (type) {
		case 'address':
			return 'Address';
		case 'bool':
			return 'Bool';
		case 'u8':
			return 'U8';
		case 'u16':
			return 'U16';
		case 'u32':
			return 'U32';
		case 'u64':
			return 'U64';
		case 'u128':
			return 'U128';
		case 'u256':
			return 'U256';
	}

	if ('vector' in type) {
		return {
			Vector: mapNormalizedType(type.vector),
		};
	}

	if ('typeParameter' in type) {
		return {
			TypeParameter: type.typeParameter,
		};
	}

	if ('datatype' in type) {
		return {
			Struct: {
				address: toShortTypeString(type.datatype.package),
				module: type.datatype.module,
				name: type.datatype.type,
				typeArguments: type.datatype.typeParameters?.map(mapNormalizedType) ?? [],
			},
		};
	}

	throw new Error('Invalid type');
}

export function mapNormalizedMoveFunction(
	fn: Rpc_Move_Function_FieldsFragment,
): SuiMoveNormalizedFunction {
	return {
		visibility: `${fn.visibility?.[0]}${fn.visibility?.slice(1).toLowerCase()}` as never,
		isEntry: fn.isEntry!,
		typeParameters:
			fn.typeParameters?.map((param) => ({
				abilities:
					param.constraints?.map(
						(constraint) =>
							`${constraint[0]}${constraint.slice(1).toLowerCase()}` as SuiMoveAbility,
					) ?? [],
			})) ?? [],
		return: fn.return?.map((param) => mapOpenMoveType(param.signature)) ?? [],
		parameters: fn.parameters?.map((param) => mapOpenMoveType(param.signature)) ?? [],
	};
}

export function mapNormalizedMoveStruct(
	struct: Rpc_Move_Struct_FieldsFragment,
): SuiMoveNormalizedStruct {
	return {
		abilities: {
			abilities:
				struct.abilities?.map(
					(ability) => `${ability[0]}${ability.slice(1).toLowerCase()}` as SuiMoveAbility,
				) ?? [],
		},
		fields:
			struct.fields?.map((field) => ({
				name: field.name,
				type: mapOpenMoveType(field.type?.signature),
			})) ?? [],
		typeParameters:
			struct.typeParameters?.map((param) => ({
				isPhantom: param.isPhantom!,
				constraints: {
					abilities: param.constraints?.map(
						(constraint) =>
							`${constraint[0]}${constraint.slice(1).toLowerCase()}` as SuiMoveAbility,
					),
				},
			})) ?? [],
	};
}

export function mapNormalizedMoveModule(
	module: Rpc_Move_Module_FieldsFragment,
	address: string,
): SuiMoveNormalizedModule {
	const exposedFunctions: Record<string, SuiMoveNormalizedFunction> = {};
	const structs: Record<string, SuiMoveNormalizedStruct> = {};

	module.functions?.nodes
		.filter((func) => func.visibility === 'PUBLIC' || func.isEntry || func.visibility === 'FRIEND')
		.forEach((func) => {
			exposedFunctions[func.name] = mapNormalizedMoveFunction(func);
		});

	module.structs?.nodes.forEach((struct) => {
		structs[struct.name] = mapNormalizedMoveStruct(struct);
	});

	return {
		address: toShortTypeString(address),
		name: module.name,
		fileFormatVersion: module.fileFormatVersion,
		friends:
			module.friends.nodes?.map((friend) => ({
				address: toShortTypeString(friend.package.address),
				name: friend.name,
			})) ?? [],
		structs,
		exposedFunctions,
	};
}

type MoveData =
	| { Address: number[] }
	| { UID: number[] }
	| { ID: number[] }
	| { Bool: boolean }
	| { Number: string }
	| { String: string }
	| { Vector: MoveData[] }
	| { Option: MoveData | null }
	| { Struct: [{ name: string; value: MoveData }] };

export type MoveTypeLayout =
	| 'address'
	| 'bool'
	| 'u8'
	| 'u16'
	| 'u32'
	| 'u64'
	| 'u128'
	| 'u256'
	| { vector: MoveTypeLayout }
	| {
			struct: {
				type: string;
				fields: { name: string; layout: MoveTypeLayout }[];
			};
	  };

export function moveDataToRpcContent(data: MoveData, layout: MoveTypeLayout): MoveValue {
	if ('Address' in data) {
		return normalizeSuiAddress(
			data.Address.map((byte) => byte.toString(16).padStart(2, '0')).join(''),
		);
	}

	if ('UID' in data) {
		return {
			id: normalizeSuiAddress(data.UID.map((byte) => byte.toString(16).padStart(2, '0')).join('')),
		};
	}

	if ('ID' in data) {
		return normalizeSuiAddress(data.ID.map((byte) => byte.toString(16).padStart(2, '0')).join(''));
	}

	if ('Bool' in data) {
		return data.Bool;
	}

	if ('Number' in data) {
		return layout === 'u64' || layout === 'u128' || layout === 'u256'
			? String(data.Number)
			: Number.parseInt(data.Number, 10);
	}

	if ('String' in data) {
		return data.String;
	}

	if ('Vector' in data) {
		if (typeof layout !== 'object' || !('vector' in layout)) {
			throw new Error(`Invalid layout for data: ${JSON.stringify(data)}}`);
		}
		const itemLayout = layout.vector;
		return data.Vector.map((item) => moveDataToRpcContent(item, itemLayout));
	}

	if ('Option' in data) {
		return data.Option && moveDataToRpcContent(data.Option, layout);
	}

	if ('Struct' in data) {
		const result: MoveStruct = {};

		if (typeof layout !== 'object' || !('struct' in layout)) {
			throw new Error(`Invalid layout for data: ${JSON.stringify(data)}}`);
		}

		data.Struct.forEach((item, index) => {
			const { name, layout: itemLayout } = layout.struct.fields[index];

			result[name] = moveDataToRpcContent(item.value, itemLayout);
		});

		// https://github.com/MystenLabs/sui/blob/5849f6845a3ab9fdb4c17523994adad461478a4c/crates/sui-json-rpc-types/src/sui_move.rs#L481
		const tag = parseStructTag(layout.struct.type);
		const structName = `${toShortTypeString(tag.address)}::${tag.module}::${tag.name}`;

		switch (structName) {
			case '0x1::string::String':
			case '0x1::ascii::String':
				return result['bytes'];
			case '0x2::url::Url':
				return result['url'];
			case '0x2::object::ID':
				return result['bytes'];
			case '0x2::object::UID':
				return {
					id: result['id'] as string,
				};
			case '0x2::balance::Balance':
				return result['value'];
			case '0x1::option::Option':
				return (result['vec'] as MoveValue[])[0] ?? null;
		}

		return {
			type: toShortTypeString(layout.struct.type),
			fields: result,
		};
	}

	throw new Error('Invalid move data: ' + JSON.stringify(data));
}
