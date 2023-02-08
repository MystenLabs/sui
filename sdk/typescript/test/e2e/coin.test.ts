// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  CoinStruct,
  LocalTxnDataSerializer,
  normalizeSuiObjectId,
  ObjectId,
  RawSigner,
  SUI_TYPE_ARG,
} from '../../src';

import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

const SPLIT_AMOUNTS = [BigInt(1), BigInt(2), BigInt(3)];

describe('Coin related API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let coinToSplit: ObjectId;
  let coinsAfterSplit: CoinStruct[];

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider),
    );
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    coinToSplit = coins[0].coinObjectId;
    // split coins into desired amount
    await signer.splitCoin({
      coinObjectId: coinToSplit,
      splitAmounts: SPLIT_AMOUNTS.map((s) => Number(s)),
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[1].coinObjectId,
    });
    coinsAfterSplit = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    expect(coinsAfterSplit.length).toEqual(coins.length + SPLIT_AMOUNTS.length);
  });

  it('test Coin utility functions', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    coins.forEach((c) => {
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
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    const coinTypeArg: string = coins[0].coinType;
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
        const balances = coins.map((c) => c.balance);
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
    expect(coins.find((c) => c.coinObjectId === coinToSplit)).toBeDefined();

    const coinsWithExclude =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
        SUI_TYPE_ARG,
        [coinToSplit],
      );
    expect(
      coinsWithExclude.find((c) => c.coinObjectId === coinToSplit),
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
        const balances = coins.map((c) => BigInt(c.balance));
        expect(balances).toStrictEqual([SPLIT_AMOUNTS[i]]);
      }),
    );
    // test multiple coins
    const allCoins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(1),
      );
    const largestBalance = BigInt(allCoins[allCoins.length - 1].balance);

    const coins =
      await toolbox.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
        toolbox.address(),
        largestBalance + SPLIT_AMOUNTS[0],
      );
    const balances = coins.map((c) => BigInt(c.balance));
    expect(balances).toStrictEqual([SPLIT_AMOUNTS[0], largestBalance]);
  });
});
