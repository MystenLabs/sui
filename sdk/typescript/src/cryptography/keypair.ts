// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../serialization/base64';
import { PublicKey } from './publickey';

/**
 * A keypair used for signing transactions.
 */
export interface Keypair {
  /**
   * The public key for this keypair
   */
  //ts-auto-guard-suppress function-type
  getPublicKey(): PublicKey;

  /**
   * Return the signature for the data
   */
  //ts-auto-guard-suppress function-type
  signData(data: Base64DataBuffer): Base64DataBuffer;
}
