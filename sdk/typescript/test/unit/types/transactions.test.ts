// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect } from 'vitest';
import mockTransactionData from '@mysten/sui-open-rpc/samples/transactions.json';

import { isSuiTransactionResponse } from '../../../src/types/index.guard';

describe('Test Transaction Definition', () => {
  it('Test against different transaction definitions', () => {
    const txns = mockTransactionData;

    expect(isSuiTransactionResponse(txns['move_call'])).toBeTruthy();
    expect(isSuiTransactionResponse(txns['transfer'])).toBeTruthy();
    expect(isSuiTransactionResponse(txns['coin_split'])).toBeTruthy();
    expect(isSuiTransactionResponse(txns['transfer_sui'])).toBeTruthy();
    // TODO: add mock data for failed transaction
    // expect(
    //   isTransactionEffectsResponse(txns['fail'])
    // ).toBeTruthy();
  });
});
