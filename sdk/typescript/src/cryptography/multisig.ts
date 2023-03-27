// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { blake2b } from '@noble/hashes/blake2b';
import { fromB64, toB64 } from '@mysten/bcs';
import { bytesEqual, PublicKeyInitData } from './publickey';
import { SerializedSignature, SIGNATURE_SCHEME_TO_FLAG } from './signature';
import { normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../types';
import { bytesToHex } from '@noble/hashes/utils';

export type SerializedMultiSig = string;
export type SerializedMultiSigPublicKey = string;

/**
 * An MultiSig public key. 
 */
export class MultiSigPublicKey {
  private data: Uint8Array;
  
  constructor(value: PublicKeyInitData) {
    if (typeof value === 'string') {
      this.data = fromB64(value);
    } else if (value instanceof Uint8Array) {
      this.data = value;
    } else {
      this.data = Uint8Array.from(value);
    }

    if (this.data.length !== PUBLIC_KEY_SIZE) {
      throw new Error(
        `Invalid public key input. Expected ${PUBLIC_KEY_SIZE} bytes, got ${this.data.length}`,
      );
    }
  }

  /**
   * Return the Sui address associated with this Ed25519 public key
   */
  toSuiAddress(): string {
    let tmp = new Uint8Array(PUBLIC_KEY_SIZE + 1);
    tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
    tmp.set(this.toBytes(), 1);
    // Each hex char represents half a byte, hence hex address doubles the length
    return normalizeSuiAddress(
      bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
    );
  }
}

export function combine_to_multisig(sigs: Array<SerializedSignature>, multisig_pk: MultiSigPublicKey): SerializedMultiSig {
  for s in sigs {
    if s.sc
  }
  let tmp = new Uint8Array(PUBLIC_KEY_SIZE + 1);
    tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
    tmp.set(this.toBytes(), 1);
    // Each hex char represents half a byte, hence hex address doubles the length
    return normalizeSuiAddress(
      bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
    );
}
