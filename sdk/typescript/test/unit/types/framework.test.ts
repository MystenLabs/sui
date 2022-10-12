// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import mockObjectData from '@mysten/sui-open-rpc/samples/objects.json';
import { Coin, GetObjectDataResponse } from '../../../src';

describe('Test framework classes', () => {
  it('Test coin utils', () => {
    const data = mockObjectData['coin'] as GetObjectDataResponse;
    expect(Coin.isCoin(data)).toBeTruthy();
    expect(Coin.getBalance(data)).toEqual(BigInt('100000000000000'));
  });
});
