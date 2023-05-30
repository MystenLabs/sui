// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toB64, toHEX } from '@mysten/bcs';
import { describe, it, expect } from 'vitest';
import { Secp256r1PublicKey } from '../../../src/cryptography/secp256r1-publickey';
import {
  INVALID_SECP256R1_PUBLIC_KEY,
  VALID_SECP256R1_PUBLIC_KEY,
} from './secp256r1-keypair.test';

// Test case generated against CLI:
// cargo build --bin sui
// ../sui/target/debug/sui client new-address secp256r1
// ../sui/target/debug/sui keytool list
let SECP_TEST_CASES = new Map<string, string>([
  [
    'AgNbPsIqEtYdkvpBRIcgfxNev/J8Suohc3b3O5a5T/X7DA==',
    '0xd135b77e2c949a104142969b2ab7f1866a1fc6882e045c0377b7f13b4532069',
  ],
]);
describe('Secp256r1PublicKey', () => {
  it('invalid', () => {
    expect(() => {
      new Secp256r1PublicKey(INVALID_SECP256R_PUBLIC_KEY);
    }).toThrow();

    expect(() => {
      const invalid_pubkey_buffer = new Uint8Array(
        INVALID_SECP256R1_PUBLIC_KEY,
      );
      let invalid_pubkey_base64 = toB64(invalid_pubkey_buffer);
      new Secp256r1PublicKey(invalid_pubkey_base64);
    }).toThrow();

    expect(() => {
      const pubkey_buffer = new Uint8Array(VALID_SECP256R1_PUBLIC_KEY);
      let wrong_encode = toHEX(pubkey_buffer);
      new Secp256r1PublicKey(wrong_encode);
    }).toThrow();

    expect(() => {
      new Secp256r1PublicKey('12345');
    }).toThrow();
  });

  it('toBase64', () => {
    const pub_key = new Uint8Array(VALID_SECP256R1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const key = new Secp256r1PublicKey(pub_key_base64);
    expect(key.toBase64()).toEqual(pub_key_base64);
    expect(key.toString()).toEqual(pub_key_base64);
  });

  it('toBuffer', () => {
    const pub_key = new Uint8Array(VALID_SECP256R1_PUBLIC_KEY);
    let pub_key_base64 = toB64(pub_key);
    const key = new Secp256r1PublicKey(pub_key_base64);
    expect(key.toBytes().length).toBe(33);
    expect(new Secp256r1PublicKey(key.toBytes()).equals(key)).toBe(true);
  });

  SECP_TEST_CASES.forEach((address, base64) => {
    it(`toSuiAddress from base64 public key ${address}`, () => {
      const key = new Secp256r1PublicKey(base64);
      expect(key.toSuiAddress()).toEqual(address);
    });
  });
});
