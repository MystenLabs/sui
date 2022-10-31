// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BN from 'bn.js';
import sha3 from 'js-sha3';
import { fromB64, toB64 } from '@mysten/bcs';
import {
  checkPublicKeyData,
  PublicKeyInitData,
  SIGNATURE_SCHEME_TO_FLAG,
} from './publickey';

const PUBLIC_KEY_SIZE = 32;

/**
 * An Ed25519 public key
 */
export class Ed25519PublicKey {
  /** @internal */
  _bn: BN;

  /**
   * Create a new Ed25519PublicKey object
   * @param value ed25519 public key as buffer or base-64 encoded string
   */
  constructor(value: PublicKeyInitData) {
    if (checkPublicKeyData(value)) {
      this._bn = value._bn;
    } else {
      if (typeof value === 'string') {
        const buffer = fromB64(value);
        if (buffer.length !== PUBLIC_KEY_SIZE) {
          throw new Error(
            `Invalid public key input. Expected ${PUBLIC_KEY_SIZE} bytes, got ${buffer.length}`
          );
        }
        this._bn = new BN(buffer);
      } else {
        this._bn = new BN(value);
      }
      let length = this._bn.byteLength();
      if (length !== PUBLIC_KEY_SIZE) {
        throw new Error(
          `Invalid public key input. Expected ${PUBLIC_KEY_SIZE} bytes, got ${length}`
        );
      }
    }
  }

  /**
   * Checks if two Ed25519 public keys are equal
   */
  equals(publicKey: Ed25519PublicKey): boolean {
    return this._bn.eq(publicKey._bn);
  }

  /**
   * Return the base-64 representation of the Ed25519 public key
   */
  toBase64(): string {
    return toB64(this.toBytes());
  }

  /**
   * Return the byte array representation of the Ed25519 public key
   */
  toBytes(): Uint8Array {
    return Uint8Array.from(this._bn.toArray());
  }

  /**
   * Return the base-64 representation of the Ed25519 public key
   */
  toString(): string {
    return this.toBase64();
  }

  /**
   * Return the Sui address associated with this Ed25519 public key
   */
  toSuiAddress(): string {
    let tmp = new Uint8Array(PUBLIC_KEY_SIZE + 1);
    tmp.set([SIGNATURE_SCHEME_TO_FLAG['ED25519']]);
    tmp.set(this.toBytes(), 1);
    return sha3.sha3_256(tmp).slice(0, 40);
  }
}
