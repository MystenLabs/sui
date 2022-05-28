// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockObjectData from '../../../../crates/sui-open-rpc/samples/objects.json';
import { Coin, GetObjectDataResponse } from '../../src';

import BN from 'bn.js';

describe('Test framework classes', () => {
  it('Test coin utils', () => {
    const data = mockObjectData['coin'] as GetObjectDataResponse;
    expect(Coin.isCoin(data)).toBeTruthy();
    expect(Coin.getBalance(data)).toEqual(new BN.BN('100000'));
  });
});
