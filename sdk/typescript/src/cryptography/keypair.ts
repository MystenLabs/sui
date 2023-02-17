// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { Ed25519Keypair } from './ed25519-keypair';
import { PublicKey } from './publickey';
import { Secp256k1Keypair } from './secp256k1-keypair';
import { SignatureScheme } from './signature';

export const PRIVATE_KEY_SIZE = 32;
export const LEGACY_PRIVATE_KEY_SIZE = 64;

export type ExportedKeypair = {
  schema: SignatureScheme;
  privateKey: string;
};

/**
 * A keypair used for signing transactions.
 */
export interface Keypair {
  /**
   * The public key for this keypair
   */
  getPublicKey(): PublicKey;

  /**
   * Return the signature for the data
   */
  signData(data: Uint8Array, useRecoverable: boolean): Uint8Array;

  /**
   * Get the key scheme of the keypair: Secp256k1 or ED25519
   */
  getKeyScheme(): SignatureScheme;

  export(): ExportedKeypair;
}

export function fromExportedKeypair(keypair: ExportedKeypair): Keypair {
  const secretKey = fromB64(keypair.privateKey);
  switch (keypair.schema) {
    case 'ED25519':
      let pureSecretKey = secretKey;
      if (secretKey.length === LEGACY_PRIVATE_KEY_SIZE) {
        // This is a legacy secret key, we need to strip the public key bytes and only read the first 32 bytes
        pureSecretKey = secretKey.slice(0, PRIVATE_KEY_SIZE);
      }
      return Ed25519Keypair.fromSecretKey(pureSecretKey);
    case 'Secp256k1':
      return Secp256k1Keypair.fromSecretKey(secretKey);
    default:
      throw new Error(`Invalid keypair schema ${keypair.schema}`);
  }
}
