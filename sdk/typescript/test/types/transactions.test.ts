// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockTransactionData from '../mocks/data/transactions.json';

import { isTransactionEffectsResponse } from '../../src/index.guard';

describe('Test Transaction Definition', () => {
  it('Test against different transaction definitions', () => {
    const txns = mockTransactionData;
    expect(isTransactionEffectsResponse(txns['move_call'])).toBeTruthy();
    expect(isTransactionEffectsResponse(txns['transfer'])).toBeTruthy();
    expect(isTransactionEffectsResponse(txns['coin_split'])).toBeTruthy();
    expect(isTransactionEffectsResponse(txns['fail'])).toBeTruthy();
  });
});
