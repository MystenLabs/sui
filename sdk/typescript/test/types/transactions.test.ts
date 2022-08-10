// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import mockTransactionData from '../../../../crates/sui-open-rpc/samples/transactions.json';

import { isSuiTransactionEffectsResponse } from '../../src/index.guard';

describe('Test Transaction Definition', () => {
  it('Test against different transaction definitions', () => {
    const txns = mockTransactionData;

    expect(isSuiTransactionEffectsResponse(txns['move_call'])).toBeTruthy();
    expect(isSuiTransactionEffectsResponse(txns['transfer'])).toBeTruthy();
    expect(isSuiTransactionEffectsResponse(txns['coin_split'])).toBeTruthy();
    expect(isSuiTransactionEffectsResponse(txns['transfer_sui'])).toBeTruthy();
    // TODO: add mock data for failed transaction
    // expect(
    //   isTransactionEffectsResponse(txns['fail'])
    // ).toBeTruthy();
  });
});
