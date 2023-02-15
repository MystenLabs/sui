// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { SerializedSignature, SignaturePubkeyPair } from '../signers/signer';
import { Ed25519PublicKey } from './ed25519-publickey';
import { Secp256k1PublicKey } from './secp256k1-publickey';

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
 * A keypair used for signing transactions.
 */
export type SignatureScheme = 'ED25519' | 'Secp256k1';

export const SIGNATURE_SCHEME_TO_FLAG = {
  ED25519: 0x00,
  Secp256k1: 0x01,
};

export const SIGNATURE_FLAG_TO_SCHEME = {
  0x00: 'ED25519',
  0x01: 'Secp256k1',
} as const;

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

export function fromSerializedSignature(
  serializedSignature: SerializedSignature,
): SignaturePubkeyPair {
  const bytes = fromB64(serializedSignature);
  const signatureScheme =
    SIGNATURE_FLAG_TO_SCHEME[bytes[0] as keyof typeof SIGNATURE_FLAG_TO_SCHEME];

  const PublicKey =
    signatureScheme === 'ED25519' ? Ed25519PublicKey : Secp256k1PublicKey;

  const signature = bytes.slice(1, bytes.length - PublicKey.SIZE);
  const pubkeyBytes = bytes.slice(1 + signature.length);
  const pubkey = new PublicKey(pubkeyBytes);

  return {
    signatureScheme,
    signature: toB64(signature),
    pubKey: pubkey.toBase64(),
  };
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
