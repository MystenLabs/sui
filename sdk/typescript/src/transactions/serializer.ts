// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BcsType } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import type { SuiMoveNormalizedType } from '../client/index.js';
import { MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS } from '../utils/index.js';
import { isValidSuiAddress } from '../utils/sui-types.js';
import type { OpenMoveTypeSignature, OpenMoveTypeSignatureBody } from './blockData/v2.js';
import { extractStructTag } from './utils.js';

const OBJECT_MODULE_NAME = 'object';
const ID_STRUCT_NAME = 'ID';

const STD_ASCII_MODULE_NAME = 'ascii';
const STD_ASCII_STRUCT_NAME = 'String';

const STD_UTF8_MODULE_NAME = 'string';
const STD_UTF8_STRUCT_NAME = 'String';

const STD_OPTION_MODULE_NAME = 'option';
const STD_OPTION_STRUCT_NAME = 'Option';

const RESOLVED_SUI_ID = {
	address: SUI_FRAMEWORK_ADDRESS,
	module: OBJECT_MODULE_NAME,
	name: ID_STRUCT_NAME,
};
const RESOLVED_ASCII_STR = {
	address: MOVE_STDLIB_ADDRESS,
	module: STD_ASCII_MODULE_NAME,
	name: STD_ASCII_STRUCT_NAME,
};
const RESOLVED_UTF8_STR = {
	address: MOVE_STDLIB_ADDRESS,
	module: STD_UTF8_MODULE_NAME,
	name: STD_UTF8_STRUCT_NAME,
};

const RESOLVED_STD_OPTION = {
	address: MOVE_STDLIB_ADDRESS,
	module: STD_OPTION_MODULE_NAME,
	name: STD_OPTION_STRUCT_NAME,
};

const isSameStruct = (a: any, b: any) =>
	a.address === b.address && a.module === b.module && a.name === b.name;

export function isTxContext(param: SuiMoveNormalizedType): boolean {
	const struct = extractStructTag(param)?.Struct;
	return (
		struct?.address === '0x2' && struct?.module === 'tx_context' && struct?.name === 'TxContext'
	);
}

function expectType(typeName: string, argVal?: unknown) {
	if (typeof argVal === 'undefined') {
		return;
	}
	if (typeof argVal !== typeName) {
		throw new Error(`Expect ${argVal} to be ${typeName}, received ${typeof argVal}`);
	}
}

const allowedTypes = ['Address', 'Bool', 'U8', 'U16', 'U32', 'U64', 'U128', 'U256'];

export function getPureSerializationType(
	normalizedType: SuiMoveNormalizedType,
	argVal: unknown,
): string | undefined {
	if (typeof normalizedType === 'string' && allowedTypes.includes(normalizedType)) {
		if (normalizedType in ['U8', 'U16', 'U32', 'U64', 'U128', 'U256']) {
			expectType('number', argVal);
		} else if (normalizedType === 'Bool') {
			expectType('boolean', argVal);
		} else if (normalizedType === 'Address') {
			expectType('string', argVal);
			if (argVal && !isValidSuiAddress(argVal as string)) {
				throw new Error('Invalid Sui Address');
			}
		}
		return normalizedType.toLowerCase();
	} else if (typeof normalizedType === 'string') {
		throw new Error(`Unknown pure normalized type ${JSON.stringify(normalizedType, null, 2)}`);
	}

	if ('Vector' in normalizedType) {
		if ((argVal === undefined || typeof argVal === 'string') && normalizedType.Vector === 'U8') {
			return 'string';
		}

		if (argVal !== undefined && !Array.isArray(argVal)) {
			throw new Error(`Expect ${argVal} to be a array, received ${typeof argVal}`);
		}

		const innerType = getPureSerializationType(
			normalizedType.Vector,
			// undefined when argVal is empty
			argVal ? argVal[0] : undefined,
		);

		if (innerType === undefined) {
			return;
		}

		return `vector<${innerType}>`;
	}

	if ('Struct' in normalizedType) {
		if (isSameStruct(normalizedType.Struct, RESOLVED_ASCII_STR)) {
			return 'string';
		} else if (isSameStruct(normalizedType.Struct, RESOLVED_UTF8_STR)) {
			return 'utf8string';
		} else if (isSameStruct(normalizedType.Struct, RESOLVED_SUI_ID)) {
			return 'address';
		} else if (isSameStruct(normalizedType.Struct, RESOLVED_STD_OPTION)) {
			const optionToVec: SuiMoveNormalizedType = {
				Vector: normalizedType.Struct.typeArguments[0],
			};
			return getPureSerializationType(optionToVec, argVal);
		}
	}

	return undefined;
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
