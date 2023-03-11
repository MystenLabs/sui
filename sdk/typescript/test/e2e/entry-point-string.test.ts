// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Commands,
  getExecutionStatusType,
  ObjectId,
  Transaction,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Test Move call with strings', () => {
  let toolbox: TestToolbox;
  let packageId: ObjectId;

  async function callWithString(str: string | string[], funcName: string) {
    const tx = new Transaction();
    tx.setGasBudget(DEFAULT_GAS_BUDGET);
    tx.add(
      Commands.MoveCall({
        target: `${packageId}::entry_point_string::${funcName}`,
        arguments: [tx.pure(str)],
      }),
    );
    const result = await toolbox.signer.signAndExecuteTransaction(tx);
    expect(getExecutionStatusType(result)).toEqual('success');
  }

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath =
      __dirname +
      '/../../../../crates/sui-core/src/unit_tests/data/entry_point_string';
    ({ packageId } = await publishPackage(packagePath));
  });

  it('Test ascii', async () => {
    await callWithString('SomeString', 'ascii_arg');
  });

  it('Test utf8', async () => {
    await callWithString('çå∞≠¢õß∂ƒ∫', 'utf8_arg');
  });

  it('Test string vec', async () => {
    await callWithString(['çå∞≠¢', 'õß∂ƒ∫'], 'utf8_vec_arg');
  });
});
