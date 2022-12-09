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
    'AkipeZGm/KGtS+abthYqMP7esvJSfmBYo4CLgH+KF7s3',
    '152de351f97b10b032c54dd7ee38729f8af117ee99943eec82a381270f73bfc0',
  ],
  [
    'Av9sIvpcwu9ChF9JmS1Of/86CgPPcypHXMCHPLp3+L7C',
    '26ad1bc7acb3600aa8e5505af2276f9201baf6ab3b1e94561bbf53c9ac8c53b0',
  ],
  [
    'Atxw6H26LOmjfP0A7aXvBsYvrEBGGV/ll18x9QgLRVU4',
    'd54e55c5001235b8821201183123e76af03cbf3a1d7ee64f0636af4210f348b3',
  ],
  [
    'AxlfPu/gPJmgPIi6fliV8HCdcOtZFylJabgY1jNoW8G6',
    'ff4620ef68cd2959678fad9d8485db98e192049b9bca71786f07ab38e0fe23d7',
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
