// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { setup, TestToolbox } from './utils/setup';

describe('Transaction Reading API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Get Total Transactions', async () => {
    const numTransactions = await toolbox.provider.getTotalTransactionNumber();
    expect(numTransactions).to.greaterThan(0);
  });

  it('Get Transaction', async () => {
    const resp = await toolbox.provider.getRecentTransactions(1);
    const digest = resp[0][1];
    const txn = await toolbox.provider.getTransactionWithEffects(digest);
    expect(txn.certificate.transactionDigest).toEqual(digest);
  });
});
