// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BN from 'bn.js';
import { Buffer } from 'buffer';
import sha3 from 'js-sha3';
import {
  checkPublicKeyData,
  PublicKey,
  PublicKeyInitData,
  SIGNATURE_SCHEME_TO_FLAG,
} from './publickey';

const SECP256K1_PUBLIC_KEY_SIZE = 33;

/**
 * A Secp256k1 public key
 */
export class Secp256k1PublicKey implements PublicKey {
  /** @internal */
  _bn: BN;

  /**
   * Create a new Secp256k1PublicKey object
   * @param value secp256k1 public key as buffer or base-64 encoded string
   */
  constructor(value: PublicKeyInitData) {
    if (checkPublicKeyData(value)) {
      this._bn = value._bn;
    } else {
      if (typeof value === 'string') {
        const buffer = Buffer.from(value, 'base64');
        if (buffer.length !== SECP256K1_PUBLIC_KEY_SIZE) {
          throw new Error(
            `Invalid public key input. Expected ${SECP256K1_PUBLIC_KEY_SIZE} bytes, got ${buffer.length}`
          );
        }
        this._bn = new BN(buffer);
      } else {
        this._bn = new BN(value);
      }
      let length = this._bn.byteLength();
      if (length != SECP256K1_PUBLIC_KEY_SIZE) {
        throw new Error(
          `Invalid public key input. Expected ${SECP256K1_PUBLIC_KEY_SIZE} bytes, got ${length}`
        );
      }
    }
  }

  /**
   * Checks if two Secp256k1 public keys are equal
   */
  equals(publicKey: Secp256k1PublicKey): boolean {
    return this._bn.eq(publicKey._bn);
  }

  /**
   * Return the base-64 representation of the Secp256k1 public key
   */
  toBase64(): string {
    return this.toBuffer().toString('base64');
  }

  /**
   * Return the byte array representation of the Secp256k1 public key
   */
  toBytes(): Uint8Array {
    return this.toBuffer();
  }

  /**
   * Return the Buffer representation of the Secp256k1 public key
   */
  toBuffer(): Buffer {
    const b = this._bn.toArrayLike(Buffer);
    if (b.length === SECP256K1_PUBLIC_KEY_SIZE) {
      return b;
    }

    const zeroPad = Buffer.alloc(SECP256K1_PUBLIC_KEY_SIZE);
    b.copy(zeroPad, SECP256K1_PUBLIC_KEY_SIZE - b.length);
    return zeroPad;
  }

  /**
   * Return the base-64 representation of the Secp256k1 public key
   */
  toString(): string {
    return this.toBase64();
  }

  /**
   * Return the Sui address associated with this Secp256k1 public key
   */
  toSuiAddress(): string {
    let tmp = new Uint8Array(SECP256K1_PUBLIC_KEY_SIZE + 1);
    tmp.set([SIGNATURE_SCHEME_TO_FLAG['Secp256k1']]);
    tmp.set(this.toBytes(), 1);
    return sha3.sha3_256(tmp).slice(0, 40);
  }
}
