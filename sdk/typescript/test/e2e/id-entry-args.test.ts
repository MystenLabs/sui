// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getExecutionStatusType, ObjectId, RawSigner } from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test ID as args to entry functions', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/id_entry_args';
    packageId = await publishPackage(signer, packagePath);
  });

  it('Test ID as arg to entry functions', async () => {
    const txn = await signer.signAndExecuteTransaction({
      kind: 'moveCall',
      data: {
        packageObjectId: packageId,
        module: 'test',
        function: 'test_id',
        typeArguments: [],
        arguments: [
          '0x000000000000000000000000c2b5625c221264078310a084df0a3137956d20ee',
        ],
        gasBudget: 2000,
      },
    });
    expect(getExecutionStatusType(txn)).toEqual('success');
  });
});
