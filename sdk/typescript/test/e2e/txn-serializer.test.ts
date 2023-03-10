// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { Commands, SUI_SYSTEM_STATE_OBJECT_ID, Transaction } from '../../src';
import { TransactionDataBuilder } from '../../src/builder/TransactionData';
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
    const deserializedTxnBuilder =
      TransactionDataBuilder.fromBytes(transactionBytes);
    const reserializedTxnBytes = await deserializedTxnBuilder.build();
    expect(reserializedTxnBytes).toEqual(transactionBytes);
  });
});
