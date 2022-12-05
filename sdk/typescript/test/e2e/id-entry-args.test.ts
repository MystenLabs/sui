// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getExecutionStatusType,
  LocalTxnDataSerializer,
  ObjectId,
  RawSigner,
} from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }, { useLocalTxnBuilder: false }])(
  'Test ID as args to entry functions',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    let packageId: ObjectId;

    beforeAll(async () => {
      toolbox = await setup();
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
      const packagePath = __dirname + '/./data/id_entry_args';
      packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);
    });

    it('Test ID as arg to entry functions', async () => {
      const txn = await signer.executeMoveCall({
        packageObjectId: packageId,
        module: 'test',
        function: 'test_id',
        typeArguments: [],
        arguments: ['sui1hexrm8m3zre03hjl5t8psga34427ply4kz29dze62w8zrkjlt9esv4rnx2'],
        gasBudget: 2000,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });
  }
);
