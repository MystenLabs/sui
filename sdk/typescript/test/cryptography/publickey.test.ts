// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey } from '../../src';

const VALID_KEY_BASE64 = 'Uz39UFseB/B38iBwjesIU1JZxY6y+TRL9P84JFw41W4=';

const BASE64_KEY_BYTES = [
  180,
  107,
  26,
  32,
  169,
  88,
  248,
  46,
  88,
  100,
  108,
  243,
  255,
  87,
  146,
  92,
  42,
  147,
  104,
  2,
  39,
  200,
  114,
  145,
  37,
  122,
  8,
  37,
  170,
  238,
  164,
  236,
];

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
    expect(key.toSuiAddress()).toEqual(
      '0828a42b0c541de0277ce21cb2cc4c451bea5aed'
    );
  });
});
