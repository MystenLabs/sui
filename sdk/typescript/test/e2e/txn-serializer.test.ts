// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  bcsForVersion,
  Commands,
  deserializeTransactionBytesToTransactionData,
  SUI_SYSTEM_STATE_OBJECT_ID,
  Transaction,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Transaction Serialization and deserialization', () => {
  let toolbox: TestToolbox;
  let packageId: string;

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath = __dirname + '/./data/serializer';
    packageId = await publishPackage(packagePath);
  });

  //   async function serializeAndDeserialize(
  //     moveCall: MoveCallTransaction,
  //   ): Promise<MoveCallTransaction> {
  //     const localTxnBytes = await localSerializer.serializeToBytes(
  //       toolbox.address(),
  //       { kind: 'moveCall', data: moveCall },
  //     );

  //     const deserialized =
  //       (await localSerializer.deserializeTransactionBytesToSignableTransaction(
  //         localTxnBytes,
  //       )) as UnserializedSignableTransaction;
  //     expect(deserialized.kind).toEqual('moveCall');

  //     const deserializedTxnData = deserializeTransactionBytesToTransactionData(
  //       bcsForVersion(await toolbox.provider.getRpcApiVersion()),
  //       localTxnBytes,
  //     );
  //     const reserialized = await localSerializer.serializeTransactionData(
  //       deserializedTxnData,
  //     );
  //     expect(reserialized).toEqual(localTxnBytes);
  //     if ('moveCall' === deserialized.kind) {
  //       const normalized = {
  //         ...deserialized.data,
  //         gasBudget: Number(deserialized.data.gasBudget!.toString(10)),
  //         gasPayment: '0x' + deserialized.data.gasPayment,
  //         gasPrice: Number(deserialized.data.gasPrice!.toString(10)),
  //       };
  //       return normalized;
  //     }

  //     throw new Error('unreachable');
  //   }

  //   it('Move Call With Type Tags', async () => {
  //     const coins = await toolbox.getGasObjectsOwnedByAddress();
  //     const moveCall = {
  //       packageObjectId: packageId,
  //       module: 'serializer_tests',
  //       function: 'list',
  //       typeArguments: ['0x2::coin::Coin<0x2::sui::SUI>', '0x2::sui::SUI'],
  //       arguments: [coins[0].objectId],
  //       gasBudget: DEFAULT_GAS_BUDGET,
  //     };
  //     await serializeAndDeserialize(moveCall);
  //   });

  it('Move Shared Object Call', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();

    const [{ suiAddress: validatorAddress }] =
      await toolbox.getActiveValidators();

    const tx = new Transaction();
    tx.add(
      Commands.MoveCall({
        target: '0x2::sui_system::request_add_delegation',
        arguments: [
          tx.input(SUI_SYSTEM_STATE_OBJECT_ID),
          tx.input(coins[2].objectId),
          tx.input(validatorAddress),
        ],
      }),
    );
    tx.setSender(await toolbox.address());
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    const transactionBytes = await tx.build({ provider: toolbox.provider });
    const deserializedTxnData = deserializeTransactionBytesToTransactionData(
      bcsForVersion(await toolbox.provider.getRpcApiVersion()),
      transactionBytes,
    );
    console.log(deserializedTxnData);

    // const deserialized = await serializeAndDeserialize(moveCall);
    // const normalized = {
    //   ...deserialized,
    //   arguments: deserialized.arguments.map((d) => '0x' + d),
    // };
    // expect(normalized).toEqual(moveCall);
  });
});
