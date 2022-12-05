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
    'rJzjxQ+FCK9m8YDU8Dq1Yx931HkIArhcw33kUPL9P8c=',
    'sui1tqdprxn9wmfm2q44m3ruthjf0dm5u6x2cdm3n2py44a57ete07gsg5xll6',
  ],
  [
    'QSLOoEwXV83ZMZu95mnJvnTxXfTdwEyWg+MeduPXmBU=',
    'sui1w9zfmw8lgxx6ngq9gc2r05yxh8c0lthws0zz72fgzmvgs8gdu4cqsdwhs2',
  ],
  [
    'iyIIV/Pje7ywljsAq31JpoyrWSQR+3s0mAVA+7uNfzo=',
    'sui1sau0w2w6j38k2tqtx0t87w9uaackz4gq5qagletswavsnc3n59ksjtk7gf',
  ],
  [
    'K6ePM4sz9MvdHEUQLz89gCa+4hImfL21Gj9ZGazu6/Q=',
    'sui1u5ymnhwverczfq5xrqc7eyxl23ysq0n33wpzds3m8t4vfmdzcrzsfawu3c',
  ],
  [
    'b0iCDMXUS8ZMJtVto1nenxYMfW539P5yRBPyISVk3Vg',
    'sui1cn6rfe7l2ngxtuwy4z2kpcaktljyghwh3c7jzevxh5w223dzpgxqz7l4hf',
  ],
]);

const VALID_KEY_BASE64 = 'Uz39UFseB/B38iBwjesIU1JZxY6y+TRL9P84JFw41W4=';

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
