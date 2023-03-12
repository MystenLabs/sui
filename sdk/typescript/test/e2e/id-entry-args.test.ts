// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Commands,
  getExecutionStatusType,
  ObjectId,
  Transaction,
} from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test ID as args to entry functions', () => {
  let toolbox: TestToolbox;
  let packageId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath = __dirname + '/./data/id_entry_args';
    ({ packageId } = await publishPackage(packagePath));
  });

  it('Test ID as arg to entry functions', async () => {
    const tx = new Transaction();
    tx.setGasBudget(2000);
    tx.add(
      Commands.MoveCall({
        target: `${packageId}::test::test_id`,
        arguments: [
          tx.pure(
            '0x000000000000000000000000c2b5625c221264078310a084df0a3137956d20ee',
          ),
        ],
      }),
    );
    const result = await toolbox.signer.signAndExecuteTransaction(tx, {
      showEffects: true,
    });
    expect(getExecutionStatusType(result)).toEqual('success');
  });
});
