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
    'A834EDsIyL2EKPl78zm6Wzw4d6KYM4KJ9t+mphDbPuGf',
    'sui1zupu00ayxqddcu3vrthm2ppe9409r504fqn7cjwl9lpmsjufqjhss6yl72',
  ],
  [
    'AxXzjBz67/6kS3gokGLq5ZMKj2I8JGisIwNkoYCakz+F',
    'sui1r8e5df4tf99jwuf6s0n8mkdauspfcq3yd3xd5twej7e2qlshwamqyt60u9',
  ],
  [
    'A1P5G6fhBC1lebdLPV7Ja7eoeIs8DKJ+uMLK2OjJrlPR',
    'sui1hexrm8m3zre03hjl5t8psga34427ply4kz29dze62w8zrkjlt9esv4rnx2',
  ],
  [
    'A55duFJFRIx9QpCkYpmpsXDNtici1OYUK9QjTKuvvezU',
    'sui1mne690jmzjda8jj34cmsd6kju5vlct88azu3z8q5l2jf7yk9f24sdu9738',
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
