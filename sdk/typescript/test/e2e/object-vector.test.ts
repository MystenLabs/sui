// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  Coin,
  getCreatedObjects,
  getExecutionStatusType,
  LocalTxnDataSerializer,
  ObjectId,
  RawSigner,
  SUI_FRAMEWORK_ADDRESS,
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
      const txn = await signer.executeMoveCall({
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
      const txn = await signer.executeMoveCall({
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

    it('Test regular arg mixed with object vector arg', async () => {
      const coins = await toolbox.provider.getGasObjectsOwnedByAddress(
        toolbox.address()
      );
      const coinIDs = coins.map((coin) => Coin.getID(coin));
      const txn = await signer.executeMoveCall({
        packageObjectId: SUI_FRAMEWORK_ADDRESS,
        module: 'pay',
        function: 'join_vec',
        typeArguments: ['0x2::sui::SUI'],
        arguments: [coinIDs[0], [coinIDs[1], coinIDs[2]]],
        gasBudget: DEFAULT_GAS_BUDGET,
      });
      expect(getExecutionStatusType(txn)).toEqual('success');
    });
  }
);
