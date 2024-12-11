// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toHex } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';

import type { TypeTag } from '../bcs/bcs.js';
import { bcs } from '../bcs/index.js';

export function deriveDynamicFieldID(
	parentId: string,
	typeTag: typeof TypeTag.$inferInput,
	key: Uint8Array,
) {
	const address = bcs.Address.serialize(parentId).toBytes();
	const tag = bcs.TypeTag.serialize(typeTag).toBytes();
	const keyLength = bcs.u64().serialize(key.length).toBytes();

	const hash = blake2b.create({
		dkLen: 32,
	});

	hash.update(new Uint8Array([0xf0]));
	hash.update(address);
	hash.update(keyLength);
	hash.update(key);
	hash.update(tag);

	return `0x${toHex(hash.digest().slice(0, 32))}`;
}
