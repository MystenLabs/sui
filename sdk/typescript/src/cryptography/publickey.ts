// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Buffer } from 'buffer';
import { sha3_256 } from 'js-sha3';

/**
 * Value to be converted into public key
 */
export type PublicKeyInitData =
  | string
  | Buffer
  | Uint8Array
  | Array<number>
  | PublicKeyData;

const zeroPadBuffer = (buffer: Uint8Array, length: number): Uint8Array => {
  const next = new Uint8Array(length);
  Buffer.from(buffer).copy(next, length - buffer.length);
  return next;
};

export const byteArrayEquals = (a: Uint8Array, b: Uint8Array): boolean => {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
};

/**
 * JSON object representation of PublicKey class
 */
export type PublicKeyData = {
  /** @internal */
  _buffer: Uint8Array;
};

export const PUBLIC_KEY_SIZE = 32;
export const TYPE_BYTE = 0x00;

export type SignatureScheme = 'ED25519' | 'Secp256k1';

const SIGNATURE_SCHEME_TO_FLAG = {
  ED25519: 0x00,
  Secp256k1: 0x01,
};

function isPublicKeyData(value: PublicKeyInitData): value is PublicKeyData {
  return (value as PublicKeyData)._buffer !== undefined;
}

/**
 * A public key
 */
export class PublicKey implements PublicKeyData {
  /** @internal */
  _buffer: Uint8Array;

  /**
   * Create a new PublicKey object
   * @param value ed25519 public key as buffer or base-64 encoded string
   */
  constructor(value: PublicKeyInitData) {
    if (isPublicKeyData(value)) {
      this._buffer = value._buffer;
    } else {
      if (typeof value === 'string') {
        const buffer = Buffer.from(value, 'base64');
        if (buffer.length !== 32) {
          throw new Error(
            `Invalid public key input. Expected 32 bytes, got ${buffer.length}`
          );
        }
        this._buffer = buffer;
      } else if (value instanceof Uint8Array) {
        this._buffer = value;
      } else if (Array.isArray(value)) {
        this._buffer = Uint8Array.from(value);
      } else {
        this._buffer = Uint8Array.from(value);
      }

      if (this._buffer.byteLength > PUBLIC_KEY_SIZE) {
        throw new Error(`Invalid public key input`);
      }
    }

    // Zero-pad to 32 bytes.
    if (this._buffer.length !== PUBLIC_KEY_SIZE) {
      this._buffer = zeroPadBuffer(this._buffer, PUBLIC_KEY_SIZE);
    }
  }

  /**
   * Checks if two publicKeys are equal
   */
  equals(publicKey: PublicKey): boolean {
    return byteArrayEquals(this._buffer, publicKey._buffer);
  }

  /**
   * Return the base-64 representation of the public key
   */
  toBase64(): string {
    return this.toBuffer().toString('base64');
  }

  /**
   * Return the byte array representation of the public key
   */
  toBytes(): Uint8Array {
    return this._buffer.slice();
  }

  /**
   * Return the Buffer representation of the public key
   */
  toBuffer(): Buffer {
    return Buffer.from(this._buffer);
  }

  /**
   * Return the base-64 representation of the public key
   */
  toString(): string {
    return this.toBase64();
  }

  /**
   * Return the Sui address associated with this public key
   */
  toSuiAddress(scheme: SignatureScheme = 'ED25519'): string {
    let tmp = new Uint8Array(PUBLIC_KEY_SIZE + 1);
    tmp.set([SIGNATURE_SCHEME_TO_FLAG[scheme]]);
    tmp.set(this.toBytes(), 1);
    // Only take the first 20 bytes
    const addressBytes = zeroPadBuffer(
      Uint8Array.from(sha3_256.digest(tmp).slice(0, 20)),
      20
    );
    return toHexString(addressBytes);
  }
}

// https://stackoverflow.com/questions/34309988/byte-array-to-hex-string-conversion-in-javascript
function toHexString(byteArray: Uint8Array) {
  return byteArray.reduce(
    (output, elem) => output + ('0' + elem.toString(16)).slice(-2),
    ''
  );
}
