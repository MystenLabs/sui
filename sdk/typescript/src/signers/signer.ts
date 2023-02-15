// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignatureScheme } from '../cryptography/publickey';

///////////////////////////////
// Exported Types

/**
 * Pair of signature and corresponding public key
 */
export type SignaturePubkeyPair = {
  signatureScheme: SignatureScheme;
  /** Base64-encoded signature */
  signature: string;
  /** Base64-encoded public key */
  pubKey: string;
};

export type SerializedSignature = string;

///////////////////////////////
// Exported Abstracts
/**
 * Serializes a transaction to a string that can be signed by a `Signer`.
 */
export interface Signer {
  // Returns the checksum address
  getAddress(): Promise<string>;

  /**
   * Returns the signature for the data and the public key of the signer
   */
  signData(data: Uint8Array): Promise<SerializedSignature>;
}
