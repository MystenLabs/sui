// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import { BCS, getSuiMoveConfig, fromB58, toB58, fromB64, toB64, fromHEX, toHEX } from './../src/index';

describe('Move bcs', () => {
  it('should de/ser hex, base58 and base64', () => {
    const bcs = new BCS(getSuiMoveConfig());

    expect(bcs.de('u8', 'AA==', 'base64')).toEqual(0);
    expect(bcs.de('u8', '00', 'hex')).toEqual(0);
    expect(bcs.de('u8', '1', 'base58')).toEqual(0);

    const STR = 'this is a test string';
    const str = bcs.ser('string', STR);

    expect(bcs.de('string', fromB58(str.toString('base58')), 'base58')).toEqual(STR);
    expect(bcs.de('string', fromB64(str.toString('base64')), 'base64')).toEqual(STR);
    expect(bcs.de('string', fromHEX(str.toString('hex')), 'hex')).toEqual(STR);
  });
});
