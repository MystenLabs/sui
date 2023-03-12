// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  normalizeSuiObjectId,
  ObjectId,
  SuiObjectInfo,
  SUI_TYPE_ARG,
  Transaction,
} from '../../src';

import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

const SPLIT_AMOUNTS = [BigInt(1), BigInt(2), BigInt(3)];

describe('Coin related API', () => {
  let toolbox: TestToolbox;
  let coinToSplit: ObjectId;
  let coinsAfterSplit: SuiObjectInfo[];

  beforeAll(async () => {
    toolbox = await setup();
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    coinToSplit = coins[0].objectId;
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    const recieverInput = tx.pure(toolbox.address());
    SPLIT_AMOUNTS.forEach((amount) => {
      const coin = tx.splitCoin(tx.gas, tx.pure(amount));
      tx.transferObjects([coin], recieverInput);
    });

    // split coins into desired amount
    await toolbox.signer.signAndExecuteTransaction(
      tx,
      {},
      'WaitForLocalExecution',
    );
    coinsAfterSplit = await toolbox.getGasObjectsOwnedByAddress();
  });

  it('test Coin utility functions', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    coins.forEach((c) => {
      expect(Coin.isCoin(c)).toBeTruthy();
      expect(Coin.isSUI(c)).toBeTruthy();
    });
  });

  it('test getCoinStructTag', async () => {
    const exampleStructTag = {
      address: normalizeSuiObjectId('0x2'),
      module: 'sui',
      name: 'SUI',
      typeParams: [],
    };
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coinTypeArg: string = Coin.getCoinTypeArg(coins[0])!;
    expect(Coin.getCoinStructTag(coinTypeArg)).toStrictEqual(exampleStructTag);
  });

  it('test selectCoinsWithBalanceGreaterThanOrEqual', async () => {
    await Promise.all(
      SPLIT_AMOUNTS.map(async (a, i) => {
        const coins =
          await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
            toolbox.address(),
            BigInt(a),
          );
        expect(coins.length).toEqual(coinsAfterSplit.length - i);
        const balances = coins.map((c) => Coin.getBalanceFromCoinStruct(c));
        // verify that the balances are in ascending order
        expect(balances).toStrictEqual(balances.sort());
        // verify that balances are all greater than or equal to the provided amount
        expect(balances.every((b) => b >= a));
      }),
    );
  });

  it('test selectCoinsWithBalanceGreaterThanOrEqual with exclude', async () => {
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
      );
    expect(
      coins.find(({ coinObjectId }) => coinObjectId === coinToSplit),
    ).toBeDefined();

    const coinsWithExclude =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
        SUI_TYPE_ARG,
        [coinToSplit],
      );
    expect(
      coinsWithExclude.find(({ coinObjectId }) => coinObjectId === coinToSplit),
    ).toBeUndefined();
  });

  it('test selectCoinSetWithCombinedBalanceGreaterThanOrEqual', async () => {
    await Promise.all(
      SPLIT_AMOUNTS.map(async (a, i) => {
        const coins =
          await toolbox.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
            toolbox.address(),
            BigInt(a),
          );
        const balances = coins.map((c) => Coin.getBalanceFromCoinStruct(c));
        expect(balances).toStrictEqual([SPLIT_AMOUNTS[i]]);
      }),
    );
    // test multiple coins
    const allCoins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
      );
    const largestBalance = Coin.getBalanceFromCoinStruct(
      allCoins[allCoins.length - 1],
    );

    const coins =
      await toolbox.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
        toolbox.address(),
        largestBalance + SPLIT_AMOUNTS[0],
      );
    const balances = coins.map((c) => Coin.getBalanceFromCoinStruct(c));
    expect(balances).toStrictEqual([SPLIT_AMOUNTS[0], largestBalance]);
  });
});
