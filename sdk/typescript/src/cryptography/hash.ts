// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { sha3_256 } from 'js-sha3';

/**
 * Generates a SHA 256 hash of typed data as a base64 string.
 *
 * @param typeTag type tag (e.g. TransactionData, SenderSignedData)
 * @param data data to hash
 */
export function sha256Hash(typeTag: string, data: Uint8Array): string {
  const hash = sha3_256.create();

  const typeTagBytes = Array.from(`${typeTag}::`).map((e) => e.charCodeAt(0));

  const dataWithTag = new Uint8Array(typeTagBytes.length + data.length);
  dataWithTag.set(typeTagBytes);
  dataWithTag.set(data, typeTagBytes.length);

  hash.update(dataWithTag);

  return Buffer.from(hash.hex(), 'hex').toString('base64');
}
