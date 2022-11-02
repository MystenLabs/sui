// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64, toHEX } from '@mysten/bcs';
import { describe, it, expect } from 'vitest';
import { Secp256k1PublicKey } from '../../../src/cryptography/secp256k1-publickey';
import {
  INVALID_SECP256K1_PUBLIC_KEY,
  VALID_SECP256K1_PUBLIC_KEY,
} from './secp256k1-keypair.test';

// Test case generated against CLI:
// cargo build --bin sui
// ../sui/target/debug/sui client new-address secp256k1
// ../sui/target/debug/sui keytool list
let SECP_TEST_CASES = new Map<string, string>([
  [
    'AwTC3jVFRxXc3RJIFgoQcv486QdqwYa8vBp4bgSq0gsI',
    '35057079b5dfc60d650768e2f4f92318f4ea5a77',
  ],
  [
    'A1F2CtldIGolO92Pm9yuxWXs5E07aX+6ZEHAnSuKOhii',
    '0187cf4234ff80862d5a1665d840df400fef29a0',
  ],
  [
    'Ak5rsa5Od4T6YFN/V3VIhZ/azMMYPkUilKQwc+RiaId+',
    '70eaff6b7973c57842c2272f00aa19af9f20dc1b',
  ],
  [
    'A4XbJ3fLvV/8ONsnLHAW1nORKsoCYsHaXv9FK1beMtvY',
    'deb28f733d9f59910cb210d56a46614f9dd28360',
  ],
]);
describe('Secp256k1PublicKey', () => {
  it('invalid', () => {
    expect(() => {
      new Secp256k1PublicKey(INVALID_SECP256K1_PUBLIC_KEY);
    }).toThrow();

    expect(() => {
      const invalid_pubkey_buffer = new Uint8Array(
        INVALID_SECP256K1_PUBLIC_KEY
      );
      let invalid_pubkey_base64 = toB64(invalid_pubkey_buffer);
      new Secp256k1PublicKey(invalid_pubkey_base64);
    }).toThrow();

    expect(() => {
      const pubkey_buffer = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
      let wrong_encode = toHEX(pubkey_buffer);
      new Secp256k1PublicKey(wrong_encode);
    }).toThrow();

    expect(() => {
      new Secp256k1PublicKey('12345');
    }).toThrow();
  });

  it('toBase64', () => {
    const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const key = new Secp256k1PublicKey(pub_key_base64);
    expect(key.toBase64()).toEqual(pub_key_base64);
    expect(key.toString()).toEqual(pub_key_base64);
  });

  it('toBuffer', () => {
    const pub_key = new Uint8Array(VALID_SECP256K1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const key = new Secp256k1PublicKey(pub_key_base64);
    expect(key.toBytes().length).toBe(33);
    expect(new Secp256k1PublicKey(key.toBytes()).equals(key)).toBe(true);
  });

  SECP_TEST_CASES.forEach((address, base64) => {
    it(`toSuiAddress from base64 public key ${address}`, () => {
      const key = new Secp256k1PublicKey(base64);
      expect(key.toSuiAddress()).toEqual(address);
    });
  });
});
