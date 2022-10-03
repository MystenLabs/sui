// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getExecutionStatusType,
  LocalTxnDataSerializer,
  RawSigner,
} from '../../src';
import {
  DEFAULT_RECIPIENT,
  DEFAULT_GAS_BUDGET,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Local Transaction Builder', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeAll(async () => {
    toolbox = await setup('fullnode');
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
  });

  it('Split coin', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.splitCoinWithRequestType({
      coinObjectId: coins[0].objectId,
      splitAmounts: [DEFAULT_GAS_BUDGET * 2],
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[1].objectId,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Merge coin', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.mergeCoinWithRequestType({
      primaryCoin: coins[0].objectId,
      coinToMerge: coins[1].objectId,
      gasBudget: DEFAULT_GAS_BUDGET,
      gasPayment: coins[2].objectId,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Move Call', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.executeMoveCallWithRequestType({
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
      gasPayment: coins[0].objectId,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Transfer Sui', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.transferSuiWithRequestType({
      suiObjectId: coins[0].objectId,
      gasBudget: DEFAULT_GAS_BUDGET,
      recipient: DEFAULT_RECIPIENT,
      amount: 100,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });

  it('Transfer Object', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    const txn = await signer.transferObjectWithRequestType({
      objectId: coins[0].objectId,
      gasBudget: DEFAULT_GAS_BUDGET,
      recipient: DEFAULT_RECIPIENT,
      gasPayment: coins[1].objectId,
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });
});
