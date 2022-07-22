// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockTransactionData from '../../../../crates/sui-open-rpc/samples/transactions.json';

import { isTransactionResponse } from '../../src/index.guard';

describe('Test Transaction Definition', () => {
  it('Test against different transaction definitions', () => {
    const txns = mockTransactionData;

    expect(isTransactionResponse(txns['move_call']['EffectResponse'])).toBeTruthy();
    expect(isTransactionResponse(txns['transfer']['EffectResponse'])).toBeTruthy();
    expect(isTransactionResponse(txns['coin_split']['SplitCoinResponse'])).toBeTruthy();
    // TODO: add mock data for failed transaction
    // expect(
    //   isTransactionEffectsResponse(txns['fail'])
    // ).toBeTruthy();
  });
});
