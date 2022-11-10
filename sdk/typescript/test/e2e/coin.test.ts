// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  getObjectId,
  LocalTxnDataSerializer,
  ObjectId,
  RawSigner,
  SuiObjectInfo,
  SUI_TYPE_ARG,
} from '../../src';

import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

const SPLIT_AMOUNTS = [BigInt(1), BigInt(2), BigInt(3)];

describe('Coin related API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let coinToSplit: ObjectId;
  let coinsAfterSplit: SuiObjectInfo[];

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    coinToSplit = coins[0].objectId;
    // split coins into desired amount
    await signer.splitCoin({
      coinObjectId: coinToSplit,
      splitAmounts: SPLIT_AMOUNTS.map((s) => Number(s)),
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[1].objectId,
    });
    coinsAfterSplit = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    expect(coinsAfterSplit.length).toEqual(coins.length + SPLIT_AMOUNTS.length);
  });

  it('test selectCoinsWithBalanceGreaterThanOrEqual', async () => {
    await Promise.all(
      SPLIT_AMOUNTS.map(async (a, i) => {
        const coins =
          await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
            toolbox.address(),
            BigInt(a)
          );
        expect(coins.length).toEqual(coinsAfterSplit.length - i);
        const balances = coins.map((c) => Coin.getBalance(c)!);
        // verify that the balances are in ascending order
        expect(balances).toStrictEqual(balances.sort());
        // verify that balances are all greater than or equal to the provided amount
        expect(balances.every((b) => b >= a));
      })
    );
  });

  it('test selectCoinsWithBalanceGreaterThanOrEqual with exclude', async () => {
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1)
      );
    expect(coins.find((c) => getObjectId(c) === coinToSplit)).toBeDefined();

    const coinsWithExclude =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
        SUI_TYPE_ARG,
        [coinToSplit]
      );
    expect(
      coinsWithExclude.find((c) => getObjectId(c) === coinToSplit)
    ).toBeUndefined();
  });

  it('test selectCoinSetWithCombinedBalanceGreaterThanOrEqual', async () => {
    await Promise.all(
      SPLIT_AMOUNTS.map(async (a, i) => {
        const coins =
          await toolbox.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
            toolbox.address(),
            BigInt(a)
          );
        const balances = coins.map((c) => Coin.getBalance(c)!);
        expect(balances).toStrictEqual([SPLIT_AMOUNTS[i]]);
      })
    );
    // test multiple coins
    const allCoins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1)
      );
    const largestBalance = Coin.getBalance(allCoins[allCoins.length - 1])!;

    const coins =
      await toolbox.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
        toolbox.address(),
        largestBalance + SPLIT_AMOUNTS[0]
      );
    const balances = coins.map((c) => Coin.getBalance(c)!);
    expect(balances).toStrictEqual([SPLIT_AMOUNTS[0], largestBalance]);
  });
});
