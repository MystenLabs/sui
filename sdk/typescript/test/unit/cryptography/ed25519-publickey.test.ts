// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { Ed25519PublicKey } from '../../../src';

// Test case generated against CLI:
// cargo build --bin sui
// ../sui/target/debug/sui client new-address ed25519
// ../sui/target/debug/sui keytool list
let TEST_CASES = new Map<string, string>([
  [
    'UdGRWooy48vGTs0HBokIis5NK+DUjiWc9ENUlcfCCBE=',
    '3415400a4bfdf924aefa55446e5f4cd6e9a9399f',
  ],
  [
    '0PTAfQmNiabgbak9U/stWZzKc5nsRqokda2qnV2DTfg=',
    '2e6dad710b343b8655825bc420783aaa5ade08c2',
  ],
  [
    '6L/l0uhGt//9cf6nLQ0+24Uv2qanX/R6tn7lWUJX1Xk=',
    '607a2403069d547c3fbba4b9e22793c7d78abb1f',
  ],
  [
    '6qZ88i8NJjaD+qZety3qXi4pLptGKS3wwO8bfDmUD+Y=',
    '7a4b0fd76cce17ef014d64ec5e073117bfc0b4de',
  ],
  [
    'RgdFhZXGe21x48rhe9X+Kh/WyFCo9ft6e9nQKZYHpi0=',
    'ecd7ef15f92a26bc8f22a88a7786fe1aae1051c6',
  ],
]);

const VALID_KEY_BASE64 = 'Uz39UFseB/B38iBwjesIU1JZxY6y+TRL9P84JFw41W4=';

const BASE64_KEY_BYTES = [
  180, 107, 26, 32, 169, 88, 248, 46, 88, 100, 108, 243, 255, 87, 146, 92, 42,
  147, 104, 2, 39, 200, 114, 145, 37, 122, 8, 37, 170, 238, 164, 236,
];

describe('Ed25519PublicKey', () => {
  it('invalid', () => {
    // public key length 33 is invalid for Ed25519
    expect(() => {
      new Ed25519PublicKey([
        3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0,
      ]);
    }).toThrow();

    expect(() => {
      new Ed25519PublicKey(
        '0x300000000000000000000000000000000000000000000000000000000000000000000'
      );
    }).toThrow();

    expect(() => {
      new Ed25519PublicKey(
        '0x300000000000000000000000000000000000000000000000000000000000000'
      );
    }).toThrow();

    expect(() => {
      new Ed25519PublicKey(
        '135693854574979916511997248057056142015550763280047535983739356259273198796800000'
      );
    }).toThrow();

    expect(() => {
      new Ed25519PublicKey('12345');
    }).toThrow();
  });

  it('toBase64', () => {
    const key = new Ed25519PublicKey(VALID_KEY_BASE64);
    expect(key.toBase64()).toEqual(VALID_KEY_BASE64);
    expect(key.toString()).toEqual(VALID_KEY_BASE64);
  });

  it('toBuffer', () => {
    const key = new Ed25519PublicKey(VALID_KEY_BASE64);
    expect(key.toBytes().length).toBe(32);
    expect(new Ed25519PublicKey(key.toBytes()).equals(key)).toBe(true);
  });

  it('toSuiAddress', () => {
    const key = new Ed25519PublicKey(new Uint8Array(BASE64_KEY_BYTES));
    expect(key.toSuiAddress()).toEqual(
      '98fc1c8179b95274327069cf3b0ed051fb14e0bc'
    );
  });

  TEST_CASES.forEach((address, base64) => {
    it(`toSuiAddress from base64 public key ${address}`, () => {
      const key = new Ed25519PublicKey(base64);
      expect(key.toSuiAddress()).toEqual(address);
    });
  });
});
