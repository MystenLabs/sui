// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  LocalTxnDataSerializer,
  RawSigner,
} from '../../src';
import {
  publishPackage,
  setup,
  TestToolbox,
} from './utils/setup';

describe.each([{ useLocalTxnBuilder: true }])(
  'Dynamic Fields Reading API',
  ({ useLocalTxnBuilder }) => {
    let toolbox: TestToolbox;
    let signer: RawSigner;
    let packageId: string;
    let shouldSkip: boolean;

    beforeAll(async () => {
      toolbox = await setup();
      const version = await toolbox.provider.getRpcApiVersion();
      shouldSkip = version?.major == 0 && version?.minor < 20;
      signer = new RawSigner(
        toolbox.keypair,
        toolbox.provider,
        useLocalTxnBuilder
          ? new LocalTxnDataSerializer(toolbox.provider)
          : undefined
      );
      const packagePath = __dirname + '/./data/dynamic_fields';
      packageId = await publishPackage(signer, useLocalTxnBuilder, packagePath);
    });

    it('get all dynamic fields', async () => {

      let obj_owned_by_address = await toolbox.provider.getObjectsOwnedByAddress(toolbox.address());
      let obj = JSON.parse(JSON.stringify(obj_owned_by_address));
      let obj_id = findDynamicObjectId(obj, packageId);

      const dynamic_fields = await toolbox.provider.getDynamicFields(
        obj_id,
        null,
        null
      );
      expect(dynamic_fields.data.length).to.toEqual(2);

    });
    it('limit response in page', async () => {

      let obj_owned_by_address = await toolbox.provider.getObjectsOwnedByAddress(toolbox.address());
      let obj = JSON.parse(JSON.stringify(obj_owned_by_address));
      let obj_id = findDynamicObjectId(obj, packageId);

      const dynamic_fields = await toolbox.provider.getDynamicFields(
        obj_id,
        null,
        1
      );
      expect(dynamic_fields.data.length).to.toEqual(1);
      expect(dynamic_fields.nextCursor).to.not.toEqual(null);

    });
    it('go to next cursor', async () => {

      let obj_owned_by_address = await toolbox.provider.getObjectsOwnedByAddress(toolbox.address());
      let obj = JSON.parse(JSON.stringify(obj_owned_by_address));
      let obj_id = findDynamicObjectId(obj, packageId);

      const dynamic_fields = await toolbox.provider.getDynamicFields(
        obj_id,
        null,
        1
      );
      console.log(dynamic_fields)
      const dynamic_fields2 = await toolbox.provider.getDynamicFields(
        obj_id,
        dynamic_fields.nextCursor,
        null
      );
      console.log(dynamic_fields2);
      expect(dynamic_fields2.data.length).greaterThan(0);
      expect(dynamic_fields.nextCursor).to.not.toEqual(null);

    });
    it('get dynamic object field', async () => {

      let obj_owned_by_address = await toolbox.provider.getObjectsOwnedByAddress(toolbox.address());
      let obj = JSON.parse(JSON.stringify(obj_owned_by_address));
      let obj_id = findDynamicObjectId(obj, packageId);

      const dynamic_fields = await toolbox.provider.getDynamicFields(
        obj_id,
        null,
        null
      );
      const obj_dof_name = dynamic_fields.data[1].name;

      const dynamic_object_field = await toolbox.provider.getDynamicFieldObject(
        obj_id,
        obj_dof_name
      )
      expect(dynamic_object_field.status).to.toEqual('Exists');

    });
  }
);

function findDynamicObjectId(
  object: JSON,
  packageId: string
){
  let obj_id = "";      
      
      let obj = JSON.parse(JSON.stringify(object));

      for (var i = 0; i< obj.length; i++){
        if(obj[i]["type"] == packageId+"::dynamic_fields_test::Test"){
          obj_id = obj[i]["objectId"];
        }
      }
      return(obj_id);
}