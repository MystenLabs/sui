// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getCreatedObjects,
  getObjectId,
  getSharedObjectInitialVersion,
  isMutableSharedObjectInput,
  isSharedObjectInput,
  ObjectId,
  SuiObjectData,
  SuiTransactionBlockResponse,
  SUI_SYSTEM_STATE_OBJECT_ID,
  TransactionBlock,
} from '../../src';
import { TransactionBlockDataBuilder } from '../../src/builder/TransactionBlockData';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Transaction Serialization and deserialization', () => {
  let toolbox: TestToolbox;
  let packageId: ObjectId;
  let publishTxn: SuiTransactionBlockResponse;
  let sharedObjectId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath = __dirname + '/./data/serializer';
    ({ packageId, publishTxn } = await publishPackage(packagePath));
    const sharedObject = getCreatedObjects(publishTxn)!.filter(
      (o) => getSharedObjectInitialVersion(o.owner) !== undefined,
    )[0];
    sharedObjectId = getObjectId(sharedObject);
  });

  async function serializeAndDeserialize(
    tx: TransactionBlock,
    mutable: boolean[],
  ) {
    console.log("start");
    tx.setSender(await toolbox.address());
    console.log("setSender");
    const transactionBlockBytes = await tx.build({
      provider: toolbox.provider,
    });
    console.log("build tx");
    const deserializedTxnBuilder = TransactionBlockDataBuilder.fromBytes(
      transactionBlockBytes,
    );
    console.log("from bytes");
    expect(
      deserializedTxnBuilder.inputs
        .filter((i) => isSharedObjectInput(i.value))
        .map((i) => isMutableSharedObjectInput(i.value)),
    ).toStrictEqual(mutable);
    console.log("finish expect");
    const reserializedTxnBytes = await deserializedTxnBuilder.build();
    console.log("Reserialize");
    expect(reserializedTxnBytes).toEqual(transactionBlockBytes);
    console.log("final check equal");
  }

  // TODO: Re-enable when this isn't broken
  it.skip('Move Shared Object Call with mutable reference', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();

    const [{ suiAddress: validatorAddress }] =
      await toolbox.getActiveValidators();

    const tx = new TransactionBlock();
    const coin = coins[2].data as SuiObjectData;
    tx.moveCall({
      target: '0x3::sui_system::request_add_stake',
      arguments: [
        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
        tx.object(coin.objectId),
        tx.pure(validatorAddress),
      ],
    });
    await serializeAndDeserialize(tx, [true]);
  });

  it.only('Move Shared Object Call with immutable reference', async () => {
    const tx = new TransactionBlock();
    tx.moveCall({
      target: `${packageId}::serializer_tests::value`,
      arguments: [tx.object(sharedObjectId)],
    });
    await serializeAndDeserialize(tx, [false]);
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
    await serializeAndDeserialize(tx, [true]);
  });
});
