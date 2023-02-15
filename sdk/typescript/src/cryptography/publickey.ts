// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519PublicKey } from './ed25519-publickey';
import { Secp256k1PublicKey } from './secp256k1-publickey';
import { SignatureScheme } from './signature';

/**
 * Value to be converted into public key.
 */
export type PublicKeyInitData = string | Uint8Array | Iterable<number>;

export function bytesEqual(a: Uint8Array, b: Uint8Array) {
  if (a === b) return true;

  if (a.length !== b.length) {
    return false;
  }

  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

/**
 * A public key
 */
export interface PublicKey {
  /**
   * Checks if two public keys are equal
   */
  equals(publicKey: PublicKey): boolean;

  /**
   * Return the base-64 representation of the public key
   */
  toBase64(): string;

  /**
   * Return the byte array representation of the public key
   */
  toBytes(): Uint8Array;

  /**
   * Return the base-64 representation of the public key
   */
  toString(): string;

  /**
   * Return the Sui address associated with this public key
   */
  toSuiAddress(): string;
}

export function publicKeyFromSerialized(
  schema: SignatureScheme,
  pubKey: string,
): PublicKey {
  if (schema === 'ED25519') {
    return new Ed25519PublicKey(pubKey);
  }
  if (schema === 'Secp256k1') {
    return new Secp256k1PublicKey(pubKey);
  }
  throw new Error('Unknown public key schema');
}
