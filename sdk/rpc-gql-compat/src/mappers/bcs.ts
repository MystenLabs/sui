// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64 } from '@mysten/bcs';
import type { BcsType } from '@mysten/bcs';
import { bcs } from '@mysten/sui.js/bcs';

import type { MoveTypeLayout } from './move.js';
import { toShortTypeString } from './util.js';

export function layoutToBcs(layout: MoveTypeLayout): BcsType<any> {
	switch (layout) {
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
	}

	if ('vector' in layout) {
		return bcs.vector(layoutToBcs(layout.vector));
	}

	if ('struct' in layout) {
		const fields: Record<string, BcsType<any>> = {};

		for (const { name, layout: field } of layout.struct.fields) {
			fields[name] = layoutToBcs(field);
		}

		let struct = bcs.struct(layout.struct.type, fields);
		const structName = toShortTypeString(layout.struct.type);

		if (structName === '0x2::object::ID') {
			struct = struct.transform({
				input: (id: any) => (typeof id === 'string' ? { bytes: id } : id) as never,
				output: (id) => id.id,
			});

			return struct;
		}
	}

	throw new Error(`Unknown layout: ${layout}`);
}

export function mapJsonToBcs(json: unknown, layout: MoveTypeLayout) {
	const schema = layoutToBcs(layout);
	return toB64(schema.serialize(json).toBytes());
}
