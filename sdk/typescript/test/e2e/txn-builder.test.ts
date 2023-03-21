// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import {
  getExecutionStatusType,
  getObjectId,
  getSharedObjectInitialVersion,
  getTransactionDigest,
  ObjectId,
  RawSigner,
  SuiTransactionResponse,
  SUI_SYSTEM_STATE_OBJECT_ID,
  Transaction,
  SuiObjectData,
  getCreatedObjects,
  SUI_CLOCK_OBJECT_ID,
} from '../../src';
import {
  DEFAULT_RECIPIENT,
  DEFAULT_GAS_BUDGET,
  setup,
  TestToolbox,
  publishPackage,
} from './utils/setup';

describe('Transaction Builders', () => {
  let toolbox: TestToolbox;
  let packageId: ObjectId;
  let publishTxn: SuiTransactionResponse;
  let sharedObjectId: ObjectId;

  beforeAll(async () => {
    const packagePath = __dirname + '/./data/serializer';
    ({ packageId, publishTxn } = await publishPackage(packagePath));
    const sharedObject = getCreatedObjects(publishTxn)!.filter(
      (o) => getSharedObjectInitialVersion(o.owner) !== undefined,
    )[0];
    sharedObjectId = getObjectId(sharedObject);
  });

  beforeEach(async () => {
    toolbox = await setup();
  });

  it('SplitCoin + TransferObjects', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new Transaction();
    const coin_0 = coins[0].details as SuiObjectData;

    const coin = tx.splitCoin(
      tx.object(coin_0.objectId),
      tx.pure(DEFAULT_GAS_BUDGET * 2),
    );
    tx.transferObjects([coin], tx.pure(toolbox.address()));
    await validateTransaction(toolbox.signer, tx);
  });

  it('MergeCoins', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coin_0 = coins[0].details as SuiObjectData;
    const coin_1 = coins[1].details as SuiObjectData;
    const tx = new Transaction();
    tx.mergeCoins(tx.object(coin_0.objectId), [tx.object(coin_1.objectId)]);
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall', async () => {
    const tx = new Transaction();
    tx.moveCall({
      target: '0x2::devnet_nft::mint',
      arguments: [
        tx.pure('Example NFT'),
        tx.pure('An NFT created by the wallet Command Line Tool'),
        tx.pure(
          'ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty',
        ),
      ],
    });
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall Shared Object', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coin_2 = coins[2].details as SuiObjectData;

    const [{ suiAddress: validatorAddress }] =
      await toolbox.getActiveValidators();

    const tx = new Transaction();
    tx.moveCall({
      target: '0x2::sui_system::request_add_stake',
      arguments: [
        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
        tx.object(coin_2.objectId),
        tx.pure(validatorAddress),
      ],
    });

    await validateTransaction(toolbox.signer, tx);
  });

  it('SplitCoin from gas object + TransferObjects', async () => {
    const tx = new Transaction();
    const coin = tx.splitCoin(tx.gas, tx.pure(1));
    tx.transferObjects([coin], tx.pure(DEFAULT_RECIPIENT));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObjects gas object', async () => {
    const tx = new Transaction();
    tx.transferObjects([tx.gas], tx.pure(DEFAULT_RECIPIENT));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObject', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new Transaction();
    const coin_0 = coins[2].details as SuiObjectData;

    tx.transferObjects(
      [tx.object(coin_0.objectId)],
      tx.pure(DEFAULT_RECIPIENT),
    );
    await validateTransaction(toolbox.signer, tx);
  });

  it('Move Shared Object Call with mixed usage of mutable and immutable references', async () => {
    const tx = new Transaction();
    tx.moveCall({
      target: `${packageId}::serializer_tests::value`,
      arguments: [tx.object(sharedObjectId)],
    });
    tx.moveCall({
      target: `${packageId}::serializer_tests::set_value`,
      arguments: [tx.object(sharedObjectId)],
    });
    await validateTransaction(toolbox.signer, tx);
  });

  it('immutable clock', async () => {
    const tx = new Transaction();
    tx.moveCall({
      target: `${packageId}::serializer_tests::use_clock`,
      arguments: [tx.object(SUI_CLOCK_OBJECT_ID)],
    });
    await validateTransaction(toolbox.signer, tx);
  });
});

async function validateTransaction(signer: RawSigner, tx: Transaction) {
  const localDigest = await signer.getTransactionDigest(tx);
  const result = await signer.signAndExecuteTransaction({
    transaction: tx,
    options: {
      showEffects: true,
    },
  });
  expect(localDigest).toEqual(getTransactionDigest(result));
  expect(getExecutionStatusType(result)).toEqual('success');
}
