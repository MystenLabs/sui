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
    'uk5FNwcG3P5Z51optEAfuJKUoytfwoRD2gLco2m0SqQ=',
    '16be190399e25dc1a62be805fd6b6007a716d6db613c63ef39e5d252ed018520',
  ],
  [
    'IeWrDXtC+DAUef25EEA6avPHFp5iXJbV97UVZ+QMWSc=',
    '500354b0b774944d83aa668aa709fa8168bdf6b5e9886d91afea3d54a081a87f',
  ],
  [
    'o4mXpCJ9+9VB6s7dbx4amjxw18840pg5Jp8tdTWuXqM=',
    'aa57a42eba21ca32437dc6fa11a1d7416b4851e31fc05d78377eae764775fa64',
  ],
  [
    'ZofmxM8S+/1HOehEzPfh7/LyLWGyZfVEMlCm3JJ/b0Q=',
    'f8e47b7ccdc3da2fa1884980493d9c3210fd15bd52a98ffce990b02a71958cdc',
  ],
  [
    'uk5FNwcG3P5Z51optEAfuJKUoytfwoRD2gLco2m0SqQ=',
    '16be190399e25dc1a62be805fd6b6007a716d6db613c63ef39e5d252ed018520',
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

  TEST_CASES.forEach((address, base64) => {
    it(`toSuiAddress from base64 public key ${address}`, () => {
      const key = new Ed25519PublicKey(base64);
      expect(key.toSuiAddress()).toEqual(address);
    });
  });
});
