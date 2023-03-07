// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { RawSigner } from '../../src';

import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('CoinRead API', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: string;
  let testType: string;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/coin_metadata';
    packageId = await publishPackage(signer, packagePath);
    testType = packageId + '::test::TEST';
  });

  it('Get coins with/without type', async () => {
    const suiCoins = await toolbox.provider.getCoins(toolbox.address());
    expect(suiCoins.data.length).toEqual(5);

    const testCoins = await toolbox.provider.getCoins(
      toolbox.address(),
      testType,
    );
    expect(testCoins.data.length).toEqual(2);

    const allCoins = await toolbox.provider.getAllCoins(toolbox.address());
    expect(allCoins.data.length).toEqual(7);
    expect(allCoins.nextCursor).toBeNull();

    //test paging with limit
    const someSuiCoins = await toolbox.provider.getCoins(
      toolbox.address(),
      null,
      null,
      3,
    );
    expect(someSuiCoins.data.length).toEqual(3);
    expect(someSuiCoins.nextCursor).toBeTruthy();
  });

  it('Get balance with/without type', async () => {
    const suiBalance = await toolbox.provider.getBalance(toolbox.address());
    expect(suiBalance.coinType).toEqual('0x2::sui::SUI');
    expect(suiBalance.coinObjectCount).toEqual(5);
    expect(suiBalance.totalBalance).toBeGreaterThan(0);

    const testBalance = await toolbox.provider.getBalance(
      toolbox.address(),
      testType,
    );
    expect(testBalance.coinType).toEqual(testType);
    expect(testBalance.coinObjectCount).toEqual(2);
    expect(testBalance.totalBalance).toEqual(11);

    const allBalances = await toolbox.provider.getAllBalances(
      toolbox.address(),
    );
    expect(allBalances.length).toEqual(2);
  });

  it('Get total supply', async () => {
    const testSupply = await toolbox.provider.getTotalSupply(testType);
    expect(testSupply.value).toEqual(11);
  });
});
