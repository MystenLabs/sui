// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { blake2b } from '@noble/hashes/blake2b';
import { fromB64, toB64 } from '@mysten/bcs';
import { bytesEqual, PublicKeyInitData } from './publickey';
import { SIGNATURE_SCHEME_TO_FLAG } from './signature';
import { normalizeSuiAddress, SUI_ADDRESS_LENGTH } from '../types';
import { bytesToHex } from '@noble/hashes/utils';

const PUBLIC_KEY_SIZE = 32;

/**
 * An Ed25519 public key
 */
export class Ed25519PublicKey {
  static SIZE = PUBLIC_KEY_SIZE;
  private data: Uint8Array;

  /**
   * Create a new Ed25519PublicKey object
   * @param value ed25519 public key as buffer or base-64 encoded string
   */
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
   * Checks if two Ed25519 public keys are equal
   */
  equals(publicKey: Ed25519PublicKey): boolean {
    return bytesEqual(this.toBytes(), publicKey.toBytes());
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
    return this.data;
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
    // Each hex char represents half a byte, hence hex address doubles the length
    return normalizeSuiAddress(
      bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
    );
  }
}
