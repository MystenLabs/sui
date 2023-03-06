// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeEach } from 'vitest';
import {
  getExecutionStatusType,
  getNewlyCreatedCoinRefsAfterSplit,
  getObjectId,
  getTransactionDigest,
  RawSigner,
  SignableTransaction,
  SUI_SYSTEM_STATE_OBJECT_ID,
} from '../../src';
import {
  DEFAULT_RECIPIENT,
  DEFAULT_GAS_BUDGET,
  setup,
  TestToolbox,
  DEFAULT_RECIPIENT_2,
} from './utils/setup';

describe('Transaction Builders', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;

  beforeEach(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
  });

  it('Split coin', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    await validateTransaction(signer, {
      kind: 'splitCoin',
      data: {
        coinObjectId: coins[0].objectId,
        splitAmounts: [DEFAULT_GAS_BUDGET * 2],
        gasBudget: DEFAULT_GAS_BUDGET,
      },
    });
  });

  it('Merge coin', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    await validateTransaction(signer, {
      kind: 'mergeCoin',
      data: {
        primaryCoin: coins[0].objectId,
        coinToMerge: coins[1].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
      },
    });
  });

  it('Move Call', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    await validateTransaction(signer, {
      kind: 'moveCall',
      data: {
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
      },
    });
  });

  it('Move Shared Object Call', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );

    const [{ sui_address: validator_address }] =
      await toolbox.getActiveValidators();

    await validateTransaction(signer, {
      kind: 'moveCall',
      data: {
        packageObjectId: '0x2',
        module: 'sui_system',
        function: 'request_add_delegation',
        typeArguments: [],
        arguments: [
          SUI_SYSTEM_STATE_OBJECT_ID,
          coins[2].objectId,
          validator_address,
        ],
        gasBudget: DEFAULT_GAS_BUDGET,
        gasPayment: coins[3].objectId,
      },
    });
  });

  it('Transfer Sui', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    await validateTransaction(signer, {
      kind: 'transferSui',
      data: {
        suiObjectId: coins[0].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipient: DEFAULT_RECIPIENT,
        amount: 100,
      },
    });
  });

  it('Transfer Object', async () => {
    const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address(),
    );
    await validateTransaction(signer, {
      kind: 'transferObject',
      data: {
        objectId: coins[0].objectId,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipient: DEFAULT_RECIPIENT,
        gasPayment: coins[1].objectId,
      },
    });
  });

  it('Pay', async () => {
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET),
      );

    // get some new coins with small amount
    const splitTxn = await signer.signAndExecuteTransaction({
      kind: 'splitCoin',
      data: {
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [1, 2, 3],
        gasBudget: DEFAULT_GAS_BUDGET,
        gasPayment: getObjectId(coins[1]),
      },
    });
    const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
      getObjectId(c),
    );

    // use the newly created coins as the input coins for the pay transaction
    await validateTransaction(signer, {
      kind: 'pay',
      data: {
        inputCoins: splitCoins,
        gasBudget: DEFAULT_GAS_BUDGET,
        recipients: [DEFAULT_RECIPIENT, DEFAULT_RECIPIENT_2],
        amounts: [4, 2],
        gasPayment: getObjectId(coins[2]),
      },
    });
  });

  it('PaySui', async () => {
    const gasBudget = 1000;
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET),
      );

    const splitTxn = await signer.signAndExecuteTransaction({
      kind: 'splitCoin',
      data: {
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [2000, 2000, 2000],
        gasBudget: gasBudget,
        gasPayment: getObjectId(coins[1]),
      },
    });
    const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
      getObjectId(c),
    );

    await validateTransaction(signer, {
      kind: 'paySui',
      data: {
        inputCoins: splitCoins,
        recipients: [DEFAULT_RECIPIENT, DEFAULT_RECIPIENT_2],
        amounts: [1000, 1000],
        gasBudget: gasBudget,
      },
    });
  });

  it('PayAllSui', async () => {
    const gasBudget = 1000;
    const coins =
      await toolbox.provider.selectCoinsWithBalanceGreaterThanOrEqual(
        toolbox.address(),
        BigInt(DEFAULT_GAS_BUDGET),
      );

    const splitTxn = await signer.signAndExecuteTransaction({
      kind: 'splitCoin',
      data: {
        coinObjectId: getObjectId(coins[0]),
        splitAmounts: [2000, 2000, 2000],
        gasBudget: gasBudget,
        gasPayment: getObjectId(coins[1]),
      },
    });
    const splitCoins = getNewlyCreatedCoinRefsAfterSplit(splitTxn)!.map((c) =>
      getObjectId(c),
    );
    await validateTransaction(signer, {
      kind: 'payAllSui',
      data: {
        inputCoins: splitCoins,
        recipient: DEFAULT_RECIPIENT,
        gasBudget: gasBudget,
      },
    });
  });
});

async function validateTransaction(
  signer: RawSigner,
  txn: SignableTransaction,
) {
  const localDigest = await signer.getTransactionDigest(txn);
  const result = await signer.signAndExecuteTransaction(txn);
  expect(localDigest).toEqual(getTransactionDigest(result));
  expect(getExecutionStatusType(result)).toEqual('success');
}
