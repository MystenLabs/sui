// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey } from '../cryptography/publickey';
import { Base64DataBuffer } from '../serialization/base64';
import { TxnDataSerializer } from './txn-data-serializers/txn-data-serializer';

///////////////////////////////
// Exported Types

/**
 * Pair of signature and corresponding public key
 */
export type SignaturePubkeyPair = {
  signature: Base64DataBuffer;
  pubKey: PublicKey;
};

///////////////////////////////
// Exported Abstracts
export abstract class Signer {
  readonly serializer?: TxnDataSerializer;

  ///////////////////
  // Sub-classes MUST implement these

  // Returns the checksum address
  abstract getAddress(): Promise<string>;

  /**
   * Returns the signature for the data and the public key of the signer
   */
  abstract signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair>;
}
