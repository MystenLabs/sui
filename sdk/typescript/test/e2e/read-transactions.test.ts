// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, vi, afterEach } from 'vitest';
import {
  getTransactionDigest,
  getTransactionKind,
  SuiTransactionBlockResponse,
  TransactionBlock,
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

  describe('waitForTransactionBlock', () => {
    async function setupTransaction() {
      const tx = new TransactionBlock();
      const [coin] = tx.splitCoins(tx.gas, [tx.pure(1)]);
      tx.transferObjects([coin], tx.pure(toolbox.address()));
      return toolbox.signer.signAndExecuteTransactionBlock({
        transactionBlock: tx,
        requestType: 'WaitForEffectsCert',
      });
    }

    afterEach(() => {
      vi.restoreAllMocks();
    });

    it('can wait for transactions with WaitForEffectsCert', async () => {
      const { digest } = await setupTransaction();

      // Should succeed using wait
      const waited = await toolbox.provider.waitForTransactionBlock({ digest });
      expect(waited.digest).toEqual(digest);
    });

    it('can be aborted using the signal', async () => {
      const { digest } = await setupTransaction();

      const abortController = new AbortController();
      abortController.abort();

      await expect(
        toolbox.provider.waitForTransactionBlock({
          digest,
          signal: abortController.signal,
        }),
      ).rejects.toThrowError();
    });

    it('times out when provided an invalid digest', async () => {
      const spy = vi
        .spyOn(toolbox.provider, 'getTransactionBlock')
        .mockImplementation(() => Promise.reject());

      await expect(
        toolbox.provider.waitForTransactionBlock({
          digest: 'foobar',
          pollInterval: 10,
          timeout: 55,
        }),
      ).rejects.toThrowError('The operation was aborted due to timeout');

      // Because JS event loop is somewhat unpredictable, we don't know exactly how long this will take, but we should have _at least_ 2 calls.
      expect(spy.mock.calls.length).toBeGreaterThan(2);
    });
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
