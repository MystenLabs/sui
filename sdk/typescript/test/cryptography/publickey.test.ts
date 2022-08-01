// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey } from '../../src';


// Sui Address                                |              Public Key (Base64)
// -------------------------------------------------------------------------------------------
// 0x4d0d03b6bf0e1a794203ac4652a2554c6efdf11a | 160QtV/5pPE2KFVC7agRpdk4AORt6QhiE5due6VYcy0=
const VALID_KEY_BASE64 = '160QtV/5pPE2KFVC7agRpdk4AORt6QhiE5due6VYcy0=';
const BASE64_KEY_BYTES = Buffer.from(VALID_KEY_BASE64, 'base64');
const EXPECTED_SUI_ADDRESS = '4d0d03b6bf0e1a794203ac4652a2554c6efdf11a'

describe('PublicKey', () => {
  it('invalid', () => {
    expect(() => {
      new PublicKey([
        3,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
      ]);
    }).toThrow();

    expect(() => {
      new PublicKey(
        '0x300000000000000000000000000000000000000000000000000000000000000000000'
      );
    }).toThrow();

    expect(() => {
      new PublicKey(
        '0x300000000000000000000000000000000000000000000000000000000000000'
      );
    }).toThrow();

    expect(() => {
      new PublicKey(
        '135693854574979916511997248057056142015550763280047535983739356259273198796800000'
      );
    }).toThrow();

    expect(() => {
      new PublicKey('12345');
    }).toThrow();
  });

  it('toBase64', () => {
    const key = new PublicKey(VALID_KEY_BASE64);
    expect(key.toBase64()).toEqual(VALID_KEY_BASE64);
    expect(key.toString()).toEqual(VALID_KEY_BASE64);
  });

  it('toBuffer', () => {
    const key = new PublicKey(VALID_KEY_BASE64);
    expect(key.toBuffer().length).toBe(32);
    expect(new PublicKey(key.toBuffer()).equals(key)).toBe(true);
  });

  it('toSuiAddress', () => {
    const key = new PublicKey(new Uint8Array(BASE64_KEY_BYTES));
    expect(key.toSuiAddress()).toEqual(EXPECTED_SUI_ADDRESS);
  });
});
