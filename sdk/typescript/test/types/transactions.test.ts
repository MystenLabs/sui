// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockTransactionData from '../mocks/data/transactions.json';

import { isCertifiedTransaction } from '../../src/index.guard';

describe('Test Transaction Definition', () => {
  it('Test against different transaction definitions', () => {
    const txns = mockTransactionData;
    expect(isCertifiedTransaction(txns['transfer'])).toBeTruthy();
    expect(isCertifiedTransaction(txns['move_call'])).toBeTruthy();
    expect(isCertifiedTransaction(txns['coin_split'])).toBeTruthy();
  });
});
