// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { blake2b } from '@noble/hashes/blake2b';

/**
 * Generates a Blake2b hash of typed data as a base64 string.
 *
 * @param typeTag type tag (e.g. TransactionData, SenderSignedData)
 * @param data data to hash
 */
export function hashTypedData(typeTag: string, data: Uint8Array): Uint8Array {
	const typeTagBytes = Array.from(`${typeTag}::`).map((e) => e.charCodeAt(0));

	const dataWithTag = new Uint8Array(typeTagBytes.length + data.length);
	dataWithTag.set(typeTagBytes);
	dataWithTag.set(data, typeTagBytes.length);

	return blake2b(dataWithTag, { dkLen: 32 });
}
