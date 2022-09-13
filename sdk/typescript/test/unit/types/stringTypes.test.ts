// Copyright (c) 2022, Mysten Labs, Inc.
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
    it('rejects non base64 strings', () => {
      expectAll(
        [
          'MDpQc 1IIzkie1dJdj nfm85XmRCJmk KHVUU05Abg==',
          'X09wJFxwQDdTU1tzMy5NJXdSTnknPCh9J0tNUCdmIw  ',
        ],
        isValidTransactionDigest,
        false
      );
    });

    it('rejects base64 strings of the wrong length', () => {
      expectAll(
        [
          'ZVteaEsxe0Q6XU53UExxWEFjKy98UD5qfmM+',
          'J3pwOz9GdS5JSEB8Lz9ILGxdJi9sXTxbdFU2OHpP',
          'UUQmaXAmQiYxSERrQH5VWEJmQm8pMXMiYEQzJ2wpPnkuYg==',
        ],
        isValidTransactionDigest,
        false
      );
    });

    it('accepts base64 strings of the correct length', () => {
      expectAll(
        [
          'UYKbz61ny/+E+r07JatGyrtrv/FyjNeqUEQisJJXPHM=',
          'obGrcB0a+aMJXyRMGQ+7to5GaJ6a1Kfd6tS+sAM0d/8=',
          'pMmQoBeSSErk96hKMtkilwCZub3FaOF3IIdii16/DBo=',
        ],
        isValidTransactionDigest,
        true
      );
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
          '9edd26f2ef1c1796f9feaa703c8628e5a70618c8',
          '5f713bef531629b47dd1bdbb382acec5224fc9ab',
          '0Xdce47e3e523b5e52a36d74295c0d83d91f80b47c',
          '0x4288ba9932cc115784794fcfb709213f30d40a54',
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
        '0x0000000000000000000000000000000000000002'
      );
    });
  });
});
