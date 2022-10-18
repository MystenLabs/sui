// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getCreatedObjects,
  getExecutionStatusType,
  LocalTxnDataSerializer,
  ObjectId,
  RawSigner,
} from '../../src';
import {
  DEFAULT_GAS_BUDGET,
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }, { useLocalTxnBuilder: false }])(
  'Test Move call with a vector of objects as input',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    let packageId: ObjectId;

    async function mintObject(val: number) {
      const txn = await signer.executeMoveCallWithRequestType({
        packageObjectId: packageId,
        module: 'entry_point_vector',
        function: 'mint',
        typeArguments: [],
        arguments: [val],
        gasBudget: DEFAULT_GAS_BUDGET,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
      return getCreatedObjects(txn)![0].reference.objectId;
    }

    async function destroyObjects(objects: ObjectId[]) {
      const txn = await signer.executeMoveCallWithRequestType({
        packageObjectId: packageId,
        module: 'entry_point_vector',
        function: 'two_obj_vec_destroy',
        typeArguments: [],
        arguments: [objects],
        gasBudget: DEFAULT_GAS_BUDGET,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    }

    beforeAll(async () => {
      toolbox = await setup();
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
      const packagePath =
        __dirname +
        '/../../../../crates/sui-core/src/unit_tests/data/entry_point_vector';
      packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);
    });

    it('Test object vector', async () => {
      await destroyObjects([await mintObject(7), await mintObject(42)]);
    });
  }
);
