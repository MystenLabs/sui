// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import {
  isValidTransactionDigest,
  isValidSuiAddress,
  normalizeSuiAddress,
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

  describe('isValidSuiAddress() / isValidObjectID()', () => {
    it('rejects non-hex strings', () => {
      expectAll(
        [
          'MDpQc 1IIzkie1dJdj nfm85XmRCJmk KHVUU05Abg==',
          'X09wJFxwQDdTU1tzMy5NJXdSTnknPCh9J0tNUCdmIw  ',
        ],
        isValidSuiAddress,
        false
      );
    });

    it('rejects hex strings of the wrong length', () => {
      expectAll(
        [
          '5f713bef531629b47dd1bdbb382a',
          'f1e2a6d12cd5e62a3ce9b2c12e9e2d37d81c',
          '0X5f713bef531629b47dd1bdbb382acec5224fc9abc16133e3',
          '0x503ff67d9291215ffccafddbd08d86e86b3425c6356c9679',
        ],
        isValidSuiAddress,
        false
      );
    });

    it('accepts hex strings of the correct length, regardless of 0x prefix', () => {
      expectAll(
        [
          '0x152de351f97b10b032c54dd7ee38729f8af117ee99943eec82a381270f73bfc0',
          '0X500354b0b774944d83aa668aa709fa8168bdf6b5e9886d91afea3d54a081a87f',
          '0xd54e55c5001235b8821201183123e76af03cbf3a1d7ee64f0636af4210f348b3',
          '0xaa57a42eba21ca32437dc6fa11a1d7416b4851e31fc05d78377eae764775fa64',
        ],
        isValidSuiAddress,
        true
      );
    });

    it('normalize hex strings to the correct length', () => {
      expectAll(
        [
          '0x2',
          '2',
          '02',
          '0X02',
          '0x0000000000000000000000000000000000000002',
          '0X000000000000000000000000000000000000002',
        ],
        normalizeSuiAddress,
        '0x0000000000000000000000000000000000000000000000000000000000000002'
      );
    });
  });
});
