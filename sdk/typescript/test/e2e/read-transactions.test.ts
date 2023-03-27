// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getTransactionDigest,
  getTransactionKind,
  SuiTransactionBlockResponse,
} from '../../src';
import { executePaySuiNTimes, setup, TestToolbox } from './utils/setup';

describe('Transaction Reading API', () => {
  let toolbox: TestToolbox;
  let transactions: SuiTransactionBlockResponse[];
  const NUM_TRANSACTIONS = 10;

  beforeAll(async () => {
    toolbox = await setup();
    transactions = await executePaySuiNTimes(toolbox.signer, NUM_TRANSACTIONS);
  });

  it('Get Total Transactions', async () => {
    const numTransactions = await toolbox.provider.getTotalTransactionBlocks();
    expect(numTransactions).toBeGreaterThan(0);
  });

  it('Get Transaction', async () => {
    const digest = transactions[0].digest;
    const txn = await toolbox.provider.getTransactionBlock({ digest });
    expect(getTransactionDigest(txn)).toEqual(digest);
  });

  it('Multi Get Pay Transactions', async () => {
    const digests = transactions.map((t) => t.digest);
    const txns = await toolbox.provider.multiGetTransactionBlocks({
      digests,
      options: { showBalanceChanges: true },
    });
    txns.forEach((txn, i) => {
      expect(getTransactionDigest(txn)).toEqual(digests[i]);
      expect(txn.balanceChanges?.length).toEqual(2);
    });
  });

  it('Query Transactions with opts', async () => {
    const options = { showEvents: true, showEffects: true };
    const resp = await toolbox.provider.queryTransactionBlocks({
      options,
      limit: 1,
    });
    const digest = resp.data[0].digest;
    const response2 = await toolbox.provider.getTransactionBlock({
      digest,
      options,
    });
    expect(resp.data[0]).toEqual(response2);
  });

  it('Get Transactions', async () => {
    const allTransactions = await toolbox.provider.queryTransactionBlocks({
      limit: 10,
    });
    expect(allTransactions.data.length).to.greaterThan(0);
  });

  it('Genesis exists', async () => {
    const allTransactions = await toolbox.provider.queryTransactionBlocks({
      limit: 1,
      order: 'ascending',
    });
    const resp = await toolbox.provider.getTransactionBlock({
      digest: allTransactions.data[0].digest,
      options: { showInput: true },
    });
    const txKind = getTransactionKind(resp)!;
    expect(txKind.kind === 'Genesis').toBe(true);
  });
});
