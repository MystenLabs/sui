// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeEach } from 'vitest';
import {
  Commands,
  getExecutionStatusType,
  getTransactionDigest,
  RawSigner,
  SUI_SYSTEM_STATE_OBJECT_ID,
  Transaction,
} from '../../src';
import {
  DEFAULT_RECIPIENT,
  DEFAULT_GAS_BUDGET,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Transaction Builders', () => {
  let toolbox: TestToolbox;

  beforeEach(async () => {
    toolbox = await setup();
  });

  it('SplitCoin + TransferObjects', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new Transaction();
    const coin = tx.add(
      Commands.SplitCoin(
        tx.input(coins[0].objectId),
        tx.input(DEFAULT_GAS_BUDGET * 2),
      ),
    );
    tx.add(Commands.TransferObjects([coin], tx.input(toolbox.address())));
    await validateTransaction(toolbox.signer, tx);
  });

  it('MergeCoins', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new Transaction();
    tx.add(
      Commands.MergeCoins(tx.input(coins[0].objectId), [
        tx.input(coins[1].objectId),
      ]),
    );
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall', async () => {
    const tx = new Transaction();
    tx.add(
      Commands.MoveCall({
        target: '0x2::devnet_nft::mint',
        typeArguments: [],
        arguments: [
          tx.input('Example NFT'),
          tx.input('An NFT created by the wallet Command Line Tool'),
          tx.input(
            'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
          ),
        ],
      }),
    );
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall Shared Object', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();

    const [{ sui_address: validator_address }] =
      await toolbox.getActiveValidators();

    const tx = new Transaction();
    tx.add(
      Commands.MoveCall({
        target: '0x2::sui_system::request_add_delegation',
        typeArguments: [],
        arguments: [
          tx.input(SUI_SYSTEM_STATE_OBJECT_ID),
          tx.input(coins[2].objectId),
          tx.input(validator_address),
        ],
      }),
    );

    await validateTransaction(toolbox.signer, tx);
  });

  it('SplitCoin from gas object + TransferObjects', async () => {
    const tx = new Transaction();
    const coin = tx.add(Commands.SplitCoin(tx.gas, tx.input(1)));
    tx.add(Commands.TransferObjects([coin], tx.input(DEFAULT_RECIPIENT)));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObjects gas object', async () => {
    const tx = new Transaction();
    tx.add(Commands.TransferObjects([tx.gas], tx.input(DEFAULT_RECIPIENT)));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObject', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new Transaction();
    tx.add(
      Commands.TransferObjects(
        [tx.input(coins[0].objectId)],
        tx.input(DEFAULT_RECIPIENT),
      ),
    );
    await validateTransaction(toolbox.signer, tx);
  });
});

async function validateTransaction(signer: RawSigner, tx: Transaction) {
  tx.setGasBudget(DEFAULT_GAS_BUDGET);
  const localDigest = await signer.getTransactionDigest(tx);
  const result = await signer.signAndExecuteTransaction(tx);
  expect(localDigest).toEqual(getTransactionDigest(result));
  expect(getExecutionStatusType(result)).toEqual('success');
}
