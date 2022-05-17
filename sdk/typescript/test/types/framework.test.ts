// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from '../../../../sui/open_rpc/samples/objects.json';
import { Coin, GetObjectInfoResponse } from '../../src';

import BN from 'bn.js';

describe('Test framework classes', () => {
  it('Test coin utils', () => {
    const data = mockObjectData['coin'] as GetObjectInfoResponse;
    expect(Coin.isCoin(data)).toBeTruthy();
    expect(Coin.getBalance(data)).toEqual(new BN.BN('100000'));
  });
});
