// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { SuiMoveNormalizedType } from '../client/index.js';
import { MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS } from '../utils/index.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { OpenMoveTypeSignature, OpenMoveTypeSignatureBody } from './data/internal.js';

const OBJECT_MODULE_NAME = 'object';
const ID_STRUCT_NAME = 'ID';

const STD_ASCII_MODULE_NAME = 'ascii';
const STD_ASCII_STRUCT_NAME = 'String';

const STD_UTF8_MODULE_NAME = 'string';
const STD_UTF8_STRUCT_NAME = 'String';

const STD_OPTION_MODULE_NAME = 'option';
const STD_OPTION_STRUCT_NAME = 'Option';

export function isTxContext(param: OpenMoveTypeSignature): boolean {
	const struct =
		typeof param.body === 'object' && 'datatype' in param.body ? param.body.datatype : null;

	return (
		!!struct &&
		normalizeSuiAddress(struct.package) === normalizeSuiAddress('0x2') &&
		struct.module === 'tx_context' &&
		struct.type === 'TxContext'
	);
}

export function getPureBcsSchema(typeSignature: OpenMoveTypeSignatureBody): BcsType<any> | null {
	if (typeof typeSignature === 'string') {
		switch (typeSignature) {
			case 'address':
				return bcs.Address;
			case 'bool':
				return bcs.Bool;
			case 'u8':
				return bcs.U8;
			case 'u16':
				return bcs.U16;
			case 'u32':
				return bcs.U32;
			case 'u64':
				return bcs.U64;
			case 'u128':
				return bcs.U128;
			case 'u256':
				return bcs.U256;
			default:
				throw new Error(`Unknown type signature ${typeSignature}`);
		}
	}

	if ('vector' in typeSignature) {
		if (typeSignature.vector === 'u8') {
			return bcs.vector(bcs.U8).transform({
				input: (val: string | Uint8Array) =>
					typeof val === 'string' ? new TextEncoder().encode(val) : val,
				output: (val) => val,
			});
		}
		const type = getPureBcsSchema(typeSignature.vector);
		return type ? bcs.vector(type) : null;
	}

	if ('datatype' in typeSignature) {
		const pkg = normalizeSuiAddress(typeSignature.datatype.package);

		if (pkg === normalizeSuiAddress(MOVE_STDLIB_ADDRESS)) {
			if (
				typeSignature.datatype.module === STD_ASCII_MODULE_NAME &&
				typeSignature.datatype.type === STD_ASCII_STRUCT_NAME
			) {
				return bcs.String;
			}

			if (
				typeSignature.datatype.module === STD_UTF8_MODULE_NAME &&
				typeSignature.datatype.type === STD_UTF8_STRUCT_NAME
			) {
				return bcs.String;
			}

			if (
				typeSignature.datatype.module === STD_OPTION_MODULE_NAME &&
				typeSignature.datatype.type === STD_OPTION_STRUCT_NAME
			) {
				const type = getPureBcsSchema(typeSignature.datatype.typeParameters[0]);
				return type ? bcs.vector(type) : null;
			}
		}

		if (
			pkg === normalizeSuiAddress(SUI_FRAMEWORK_ADDRESS) &&
			typeSignature.datatype.module === OBJECT_MODULE_NAME &&
			typeSignature.datatype.type === ID_STRUCT_NAME
		) {
			return bcs.Address;
		}
	}

	return null;
}

export function normalizedTypeToMoveTypeSignature(
	type: SuiMoveNormalizedType,
): OpenMoveTypeSignature {
	if (typeof type === 'object' && 'Reference' in type) {
		return {
			ref: '&',
			body: normalizedTypeToMoveTypeSignatureBody(type.Reference),
		};
	}
	if (typeof type === 'object' && 'MutableReference' in type) {
		return {
			ref: '&mut',
			body: normalizedTypeToMoveTypeSignatureBody(type.MutableReference),
		};
	}

	return {
		ref: null,
		body: normalizedTypeToMoveTypeSignatureBody(type),
	};
}

function normalizedTypeToMoveTypeSignatureBody(
	type: SuiMoveNormalizedType,
): OpenMoveTypeSignatureBody {
	if (typeof type === 'string') {
		switch (type) {
			case 'Address':
				return 'address';
			case 'Bool':
				return 'bool';
			case 'U8':
				return 'u8';
			case 'U16':
				return 'u16';
			case 'U32':
				return 'u32';
			case 'U64':
				return 'u64';
			case 'U128':
				return 'u128';
			case 'U256':
				return 'u256';
			default:
				throw new Error(`Unexpected type ${type}`);
		}
	}

	if ('Vector' in type) {
		return { vector: normalizedTypeToMoveTypeSignatureBody(type.Vector) };
	}

	if ('Struct' in type) {
		return {
			datatype: {
				package: type.Struct.address,
				module: type.Struct.module,
				type: type.Struct.name,
				typeParameters: type.Struct.typeArguments.map(normalizedTypeToMoveTypeSignatureBody),
			},
		};
	}

	if ('TypeParameter' in type) {
		return { typeParameter: type.TypeParameter };
	}

	throw new Error(`Unexpected type ${JSON.stringify(type)}`);
}

export function pureBcsSchemaFromOpenMoveTypeSignatureBody(
	typeSignature: OpenMoveTypeSignatureBody,
): BcsType<any> {
	if (typeof typeSignature === 'string') {
		switch (typeSignature) {
			case 'address':
				return bcs.Address;
			case 'bool':
				return bcs.Bool;
			case 'u8':
				return bcs.U8;
			case 'u16':
				return bcs.U16;
			case 'u32':
				return bcs.U32;
			case 'u64':
				return bcs.U64;
			case 'u128':
				return bcs.U128;
			case 'u256':
				return bcs.U256;
			default:
				throw new Error(`Unknown type signature ${typeSignature}`);
		}
	}

	if ('vector' in typeSignature) {
		return bcs.vector(pureBcsSchemaFromOpenMoveTypeSignatureBody(typeSignature.vector));
	}

	throw new Error(`Expected pure typeSignature, but got ${JSON.stringify(typeSignature)}`);
}
