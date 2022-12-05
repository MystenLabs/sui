// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import {
  isValidTransactionDigest,
  isValidSuiAddress,
} from '../../../src/index';

describe('String type guards', () => {
  function expectAll<T>(data: T[], check: (value: T) => any, expected: any) {
    data.forEach((d) => expect(check(d)).toBe(expected));
  }

  describe('isValidTransactionDigest()', () => {
    it('rejects invalid base64', () => {
      expect(isValidTransactionDigest('MDpQc 1IIzkie1dJdj nfm85XmRCJmk KHVUU05Abg==', 'base64')).toBe(false);
      expect(isValidTransactionDigest('X09wJFxwQDdTU1tzMy5NJXdSTnknPCh9J0tNUCdmIw  ', 'base64')).toBe(false);
    });

    it('rejects base64 string of wrong length', () => {
      expect(isValidTransactionDigest('c3Nz', 'base64')).toBe(false);
      expect(isValidTransactionDigest('MTExMQ==', 'base64')).toBe(false);
    });

    it('accepts base64 strings of correct length', () => {
      expect(isValidTransactionDigest('UYKbz61ny/+E+r07JatGyrtrv/FyjNeqUEQisJJXPHM=', 'base64')).toBe(true);
      expect(isValidTransactionDigest('obGrcB0a+aMJXyRMGQ+7to5GaJ6a1Kfd6tS+sAM0d/8=', 'base64')).toBe(true);
      expect(isValidTransactionDigest('pMmQoBeSSErk96hKMtkilwCZub3FaOF3IIdii16/DBo=', 'base64')).toBe(true);
    });

    it('rejects base58 strings of the wrong length', () => {
      expect(isValidTransactionDigest('r', 'base58')).toBe(false);
      expect(isValidTransactionDigest('HXLk', 'base58')).toBe(false);
      expect(isValidTransactionDigest('3mJ6x8dSE2KLrk', 'base58')).toBe(false);
    });

    it('accepts base58 strings of the correct length', () => {
      expect(isValidTransactionDigest('vQMG8nrGirX14JLfyzy15DrYD3gwRC1eUmBmBzYUsgh', 'base58')).toBe(true);
      expect(isValidTransactionDigest('7msXn7aieHy73WkRxh3Xdqh9PEoPYBmJW59iE4TVvz62', 'base58')).toBe(true);
      expect(isValidTransactionDigest('C6G8PsqwNpMqrK7ApwuQUvDgzkFcUaUy6Y5ycrAN2q3F', 'base58')).toBe(true);
    });
  });

  describe('isValidSuiAddress', () => {
    it('rejects invalid address', () => {
      expectAll(
        ['MDpQc 1IIzkie1dJdj nfm85XmRCJmk KHVUU05Abg==', // base64
        '0x0000000000000000000000000000000000000000000000000000000000000000', // hex of 32 bytes
        '0x0000000000000000000000000000000000000000', // hex of 20 bytes
        '0000000000000000000000000000000000000000000000000000000000000000', // hex of 32 bytes no 0x prefix
        '0000000000000000000000000000000000000000', // hex of 20 bytes no 0x prefix
        'sui1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqg40c04', // bech32 of 20 bytes (incorrect length)
        'bc1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqaj76hn', // bech32 with 32 bytes with wrong hrp
        'sui1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqgzlz8e', // bech32m (wrong scheme) with 32 bytes
        ],
        isValidSuiAddress,
        false
      );
    });

    it('accepts string with sui prefix and of correct length', () => {
      expectAll(
        [
          'sui1hexrm8m3zre03hjl5t8psga34427ply4kz29dze62w8zrkjlt9esv4rnx2',
          'sui1mne690jmzjda8jj34cmsd6kju5vlct88azu3z8q5l2jf7yk9f24sdu9738',
          'sui1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqa70wzm'
        ],
        isValidSuiAddress,
        true
      );
    });
  });
});
