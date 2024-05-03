// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { splitGenericParameters } from '@mysten/bcs';

import { normalizeSuiAddress } from '../utils/sui-types.js';
import type { TypeTag } from './types.js';

const VECTOR_REGEX = /^vector<(.+)>$/;
const STRUCT_REGEX = /^([^:]+)::([^:]+)::([^<]+)(<(.+)>)?/;

export class TypeTagSerializer {
	static parseFromStr(str: string, normalizeAddress = false): TypeTag {
		if (str === 'address') {
			return { address: null };
		} else if (str === 'bool') {
			return { bool: null };
		} else if (str === 'u8') {
			return { u8: null };
		} else if (str === 'u16') {
			return { u16: null };
		} else if (str === 'u32') {
			return { u32: null };
		} else if (str === 'u64') {
			return { u64: null };
		} else if (str === 'u128') {
			return { u128: null };
		} else if (str === 'u256') {
			return { u256: null };
		} else if (str === 'signer') {
			return { signer: null };
		}

		const vectorMatch = str.match(VECTOR_REGEX);
		if (vectorMatch) {
			return {
				vector: TypeTagSerializer.parseFromStr(vectorMatch[1], normalizeAddress),
			};
		}

		const structMatch = str.match(STRUCT_REGEX);
		if (structMatch) {
			const address = normalizeAddress ? normalizeSuiAddress(structMatch[1]) : structMatch[1];
			return {
				struct: {
					address,
					module: structMatch[2],
					name: structMatch[3],
					typeParams:
						structMatch[5] === undefined
							? []
							: TypeTagSerializer.parseStructTypeArgs(structMatch[5], normalizeAddress),
				},
			};
		}

		throw new Error(`Encountered unexpected token when parsing type args for ${str}`);
	}

	static parseStructTypeArgs(str: string, normalizeAddress = false): TypeTag[] {
		return splitGenericParameters(str).map((tok) =>
			TypeTagSerializer.parseFromStr(tok, normalizeAddress),
		);
	}

	static tagToString(tag: TypeTag): string {
		if ('bool' in tag) {
			return 'bool';
		}
		if ('u8' in tag) {
			return 'u8';
		}
		if ('u16' in tag) {
			return 'u16';
		}
		if ('u32' in tag) {
			return 'u32';
		}
		if ('u64' in tag) {
			return 'u64';
		}
		if ('u128' in tag) {
			return 'u128';
		}
		if ('u256' in tag) {
			return 'u256';
		}
		if ('address' in tag) {
			return 'address';
		}
		if ('signer' in tag) {
			return 'signer';
		}
		if ('vector' in tag) {
			return `vector<${TypeTagSerializer.tagToString(tag.vector)}>`;
		}
		if ('struct' in tag) {
			const struct = tag.struct;
			const typeParams = struct.typeParams.map(TypeTagSerializer.tagToString).join(', ');
			return `${struct.address}::${struct.module}::${struct.name}${
				typeParams ? `<${typeParams}>` : ''
			}`;
		}
		throw new Error('Invalid TypeTag');
	}
}
