// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { LocalTxnDataSerializer, RawSigner } from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }])(
  'Dynamic Fields Reading API',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    let packageId: string;
    let parent_objectID: string;

    beforeAll(async () => {
      toolbox = await setup();
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
      const packagePath = __dirname + '/./data/dynamic_fields';
      packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);

      const objs_owned_by_address =
        await toolbox.provider.getObjectsOwnedByAddress(toolbox.address());
      const obj = objs_owned_by_address.filter(
        (o) => o.type === `${packageId}::dynamic_fields_test::Test`
      );
      parent_objectID = obj[0].objectId;
    });

    it('get all dynamic fields', async () => {
      const dynamic_fields = await toolbox.provider.getDynamicFields(
        parent_objectID,
        null,
        null
      );
      expect(dynamic_fields.data.length).toEqual(2);
    });
    it('limit response in page', async () => {
      const dynamic_fields = await toolbox.provider.getDynamicFields(
        parent_objectID,
        null,
        1
      );
      expect(dynamic_fields.data.length).toEqual(1);
      expect(dynamic_fields.nextCursor).not.toEqual(null);
    });
    it('go to next cursor', async () => {
      const dynamic_fields = await toolbox.provider.getDynamicFields(
        parent_objectID,
        null,
        1
      );
      const dynamic_fields2 = await toolbox.provider.getDynamicFields(
        parent_objectID,
        dynamic_fields.nextCursor,
        null
      );
      expect(dynamic_fields2.data.length).greaterThan(0);
      expect(dynamic_fields.nextCursor).not.toEqual(null);
    });
    it('get dynamic object field', async () => {
      const dynamic_fields = await toolbox.provider.getDynamicFields(
        parent_objectID,
        null,
        null
      );
      const obj_dof_name = dynamic_fields.data[1].name;

      const dynamic_object_field = await toolbox.provider.getDynamicFieldObject(
        parent_objectID,
        obj_dof_name
      );
      expect(dynamic_object_field.status).toEqual('Exists');
    });
  }
);
