// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { Commands, SUI_SYSTEM_STATE_OBJECT_ID, Transaction } from '../../src';
import { TransactionDataBuilder } from '../../src/builder/TransactionData';
import { DEFAULT_GAS_BUDGET, setup, TestToolbox } from './utils/setup';

describe('Transaction Serialization and deserialization', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Move Shared Object Call', async () => {
    const coins = await toolbox.getGasObjectsOwnedByAddress();

    const [{ suiAddress: validatorAddress }] =
      await toolbox.getActiveValidators();

    const tx = new Transaction();
    tx.add(
      Commands.MoveCall({
        target: '0x2::sui_system::request_add_stake',
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
