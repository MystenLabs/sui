// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import type { SuiClient } from '@mysten/sui.js/client';
import type { Signature } from './types';

/**
 * Checks if an object is "Immutable" by looking up its data on the blockchain.
 * @param objectId - The ID of the object to check.
 * @param client - The SuiClient instance to use for the API request.
 * @returns A Promise that resolves to a boolean indicating whether the object is owned by an "Immutable" owner.
 * @throws An error if the "owner" field of the object cannot be extracted.
 */
export async function isImmutable(objectId: string, client: SuiClient) {
  const obj = await client.getObject({
    id: objectId,
    options: {
      showOwner: true,
    },
  });
  const objectOwner = obj?.data?.owner;
  if (!objectOwner) {
    throw new Error(`Could not extract "owner" field of object ${objectId}`);
  }
  return objectOwner == 'Immutable';
}

/**
 * Checks if the given object type is a coin.
 * Defaults to checking if the object type is a SUI (gas) coin.
 * @param objectType The object type to check.
 * @param ofType The expected object type.
 * @returns True if the object type is a coin, false otherwise.
 */
export function isCoin(
  objectType: string,
  ofType = '0x2::coin::Coin<0x2::sui::SUI>',
) {
  return objectType === ofType;
}

/**
 * Checks if the given input is of Signature type or not.
 * @param signature The input to check.
 * @returns True if the input is of Signature type, false otherwise.
 */
export function isSignature(signature: any): signature is Signature {
  return typeof signature === 'string';
}
