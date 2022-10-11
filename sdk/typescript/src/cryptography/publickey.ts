// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import BN from 'bn.js';

/**
 * Value to be converted into public key.
 */
export type PublicKeyInitData =
  | number
  | string
  | Buffer
  | Uint8Array
  | Array<number>
  | PublicKeyData;

/**
 * JSON object representation of PublicKey class.
 */
export type PublicKeyData = {
  /** @internal */
  _bn: BN;
};

/**
 * A keypair used for signing transactions.
 */
export type SignatureScheme = 'ED25519' | 'Secp256k1';

export const SIGNATURE_SCHEME_TO_FLAG = {
  ED25519: 0x00,
  Secp256k1: 0x01,
};

export function checkPublicKeyData(
  value: PublicKeyInitData
): value is PublicKeyData {
  return (value as PublicKeyData)._bn !== undefined;
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
   * Return the Buffer representation of the public key
   */
  toBuffer(): Buffer;

  /**
   * Return the base-64 representation of the public key
   */
  toString(): string;

  /**
   * Return the Sui address associated with this public key
   */
  toSuiAddress(): string;
}
