// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getExecutionStatusType, ObjectId, RawSigner } from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe('Test Move call with strings', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;

  async function callWithString(str: string | string[], funcName: string) {
    const txn = await signer.signAndExecuteTransaction({
      kind: 'moveCall',
      data: {
        packageObjectId: packageId,
        module: 'entry_point_string',
        function: funcName,
        typeArguments: [],
        arguments: [str],
        gasBudget: DEFAULT_GAS_BUDGET,
      },
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  }

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath =
      __dirname +
      '/../../../../crates/sui-core/src/unit_tests/data/entry_point_string';
    packageId = await publishPackage(signer, packagePath);
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
