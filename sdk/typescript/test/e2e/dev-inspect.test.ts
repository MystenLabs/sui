// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
    getObjectId,
    getNewlyCreatedCoinRefsAfterSplit,
    LocalTxnDataSerializer,
    SignableTransaction,
    RawSigner,
  } from '../../src';
import {
    DEFAULT_GAS_BUDGET,
    DEFAULT_RECIPIENT,
    setup,
    TestToolbox,
  } from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }, { useLocalTxnBuilder: false }])(
    'Test dev inspect a payAllSui txn',
    ({ useLocalTxnBuilder }) => {
        let toolbox: TestToolbox;
        let signer: RawSigner;

        beforeAll(async () => {
            toolbox = await setup();
            signer = new RawSigner(
              toolbox.keypair,
              toolbox.provider,
              useLocalTxnBuilder
                ? new LocalTxnDataSerializer(toolbox.provider)
                : undefined
            );
        });

        it('Dev inspect transaction', async () => {
            const gasBudget = 1000;
      const coins =
        await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
          toolbox.address(),
          BigInt(DEFAULT_GAS_BUDGET)
        );

      const splitTxn = await signer.splitCoin({
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [2000, 2000, 2000],
        gasBudget: gasBudget,
        gasPayment: getObjectId(coins[1]),
      });
      const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
        getObjectId(c)
      );

      await validateDevInspectTransaction(signer, {
        kind: 'payAllSui',
        data: {
          inputCoins: splitCoins,
          recipient: DEFAULT_RECIPIENT,
          gasBudget: gasBudget,
        },
        }
      );
        });
    }
);

async function validateDevInspectTransaction(
    signer: RawSigner,
    txn: SignableTransaction
  ) {
    const localDigest = await signer.getTransactionDigest(txn);
    const result = await signer.devInspectTransaction(txn);
    expect(localDigest).toEqual(result.effects.transactionDigest);
    expect(result.effects.status).toEqual('success');
  }
