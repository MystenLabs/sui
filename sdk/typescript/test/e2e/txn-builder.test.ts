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
  SuiTransactionBlockResponse,
  SUI_SYSTEM_STATE_OBJECT_ID,
  TransactionBlock,
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
  let publishTxn: SuiTransactionBlockResponse;
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

  it('SplitCoins + TransferObjects', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new TransactionBlock();
    const coin_0 = coins[0].data as SuiObjectData;

    const coin = tx.splitCoins(tx.object(coin_0.objectId), [
      tx.pure(DEFAULT_GAS_BUDGET * 2),
    ]);
    tx.transferObjects([coin], tx.pure(toolbox.address()));
    await validateTransaction(toolbox.signer, tx);
  });

  it('MergeCoins', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coin_0 = coins[0].data as SuiObjectData;
    const coin_1 = coins[1].data as SuiObjectData;
    const tx = new TransactionBlock();
    tx.mergeCoins(tx.object(coin_0.objectId), [tx.object(coin_1.objectId)]);
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coin_0 = coins[0].data as SuiObjectData;
    const tx = new TransactionBlock();
    tx.moveCall({
      target: '0x2::pay::split',
      typeArguments: ['0x2::sui::SUI'],
      arguments: [tx.object(coin_0.objectId), tx.pure(DEFAULT_GAS_BUDGET * 2)],
    });
    await validateTransaction(toolbox.signer, tx);
  });

  it('MoveCall Shared Object', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const coin_2 = coins[2].data as SuiObjectData;

    const [{ suiAddress: validatorAddress }] =
      await toolbox.getActiveValidators();

    const tx = new TransactionBlock();
    tx.moveCall({
      target: '0x3::sui_system::request_add_stake',
      arguments: [
        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
        tx.object(coin_2.objectId),
        tx.pure(validatorAddress),
      ],
    });

    await validateTransaction(toolbox.signer, tx);
  });

  it('SplitCoins from gas object + TransferObjects', async () => {
    const tx = new TransactionBlock();
    const coin = tx.splitCoins(tx.gas, [tx.pure(1)]);
    tx.transferObjects([coin], tx.pure(DEFAULT_RECIPIENT));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObjects gas object', async () => {
    const tx = new TransactionBlock();
    tx.transferObjects([tx.gas], tx.pure(DEFAULT_RECIPIENT));
    await validateTransaction(toolbox.signer, tx);
  });

  it('TransferObject', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();
    const tx = new TransactionBlock();
    const coin_0 = coins[2].data as SuiObjectData;

    tx.transferObjects(
      [tx.object(coin_0.objectId)],
      tx.pure(DEFAULT_RECIPIENT),
    );
    await validateTransaction(toolbox.signer, tx);
  });

  it('Move Shared Object Call with mixed usage of mutable and immutable references', async () => {
    const tx = new TransactionBlock();
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
    const tx = new TransactionBlock();
    tx.moveCall({
      target: `${packageId}::serializer_tests::use_clock`,
      arguments: [tx.object(SUI_CLOCK_OBJECT_ID)],
    });
    await validateTransaction(toolbox.signer, tx);
  });
});

async function validateTransaction(signer: RawSigner, tx: TransactionBlock) {
  const localDigest = await signer.getTransactionBlockDigest(tx);
  const result = await signer.signAndExecuteTransactionBlock({
    transactionBlock: tx,
    options: {
      showEffects: true,
    },
  });
  expect(localDigest).toEqual(getTransactionDigest(result));
  expect(getExecutionStatusType(result)).toEqual('success');
}
