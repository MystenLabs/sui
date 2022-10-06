// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  getExecutionStatusType,
  getNewlyCreatedCoinsAfterSplit,
  getObjectId,
  RawSigner,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  DEFAULT_RECIPIENT,
  DEFAULT_RECIPIENT_2,
  setup,
  TestToolbox,
} from './utils/setup';

describe('RPC Transaction Builder', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup('gateway');
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    await signer.syncAccountState();
  });

  it('Split coin', async () => {
    const coins = await toolbox.provider.getCoinBalancesOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.splitCoin({
      coinObjectId: getObjectId(coins[0]),
      splitAmounts: [DEFAULT_GAS_BUDGET],
      gasBudget: DEFAULT_GAS_BUDGET,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Merge coin', async () => {
    const coins = await toolbox.provider.getCoinBalancesOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.mergeCoin({
      primaryCoin: getObjectId(coins[0]),
      coinToMerge: getObjectId(coins[1]),
      gasBudget: DEFAULT_GAS_BUDGET,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Move Call', async () => {
    const txn = await signer.executeMoveCall({
      packageObjectId: '0x2',
      module: 'devnet_nft',
      function: 'mint',
      typeArguments: [],
      arguments: [
        'Example NFT',
        'An NFT created by the wallet Command Line Tool',
        'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
      ],
      gasBudget: DEFAULT_GAS_BUDGET,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Transfer Object', async () => {
    const coins = await toolbox.provider.getCoinBalancesOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.transferObject({
      objectId: getObjectId(coins[0]),
      gasBudget: DEFAULT_GAS_BUDGET,
      recipient: DEFAULT_RECIPIENT,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Transfer Sui', async () => {
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET)
      );
    const txn = await signer.transferSui({
      suiObjectId: getObjectId(coins[0]),
      gasBudget: DEFAULT_GAS_BUDGET,
      recipient: DEFAULT_RECIPIENT,
      amount: null,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Pay', async () => {
    const coins = await toolbox.provider.getCoinBalancesOwnedByAddress(
      toolbox.address()
    );

    // get some new coins with small amount
    const splitTxn = await signer.splitCoin({
      coinObjectId: getObjectId(coins[0]),
      splitAmounts: [1, 2, 3],
      gasBudget: DEFAULT_GAS_BUDGET,
    });
    const splitCoins = getNewlyCreatedCoinsAfterSplit(splitTxn)!.map((c) =>
      getObjectId(c.reference)
    );

    // use the newly created coins as the input coins for the pay transaction
    const txn = await signer.pay({
      inputCoins: splitCoins,
      gasBudget: DEFAULT_GAS_BUDGET,
      recipients: [DEFAULT_RECIPIENT, DEFAULT_RECIPIENT_2],
      amounts: [4, 2],
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });
});
