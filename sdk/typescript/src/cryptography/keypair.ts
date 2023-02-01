// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { Base64DataBuffer } from '../serialization/base64';
import { Ed25519Keypair } from './ed25519-keypair';
import { PublicKey, SignatureScheme } from './publickey';
import { Secp256k1Keypair } from './secp256k1-keypair';

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
  signData(data: Base64DataBuffer, useRecoverable: boolean): Base64DataBuffer;

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
      return Ed25519Keypair.fromSecretKey(secretKey);
    case 'Secp256k1':
      return Secp256k1Keypair.fromSecretKey(secretKey);
    default:
      throw new Error(`Invalid keypair schema ${keypair.schema}`);
  }
}
