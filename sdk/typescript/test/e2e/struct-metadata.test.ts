// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { publishPackage, setup, TestToolbox } from './utils/setup';
import { RawSigner, ObjectId, LocalTxnDataSerializer } from '../../src';

describe('Test Struct Metadata', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;
  let objectId: string;
  let address: string;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );

    const packagePath = __dirname + '/./data/struct_metadata';
    packageId = await publishPackage(signer, true, packagePath);
    address = await signer.getAddress();
  });

  it('Test accessing struct metadata', async () => {

    const structMetadata = await toolbox.provider.getObjectsOwnedByAddress(
      address
    );

    for (let object of structMetadata){
      if (object.type == `${packageId}::struct_metadata::Dummy`){
        objectId = object.objectId;
      }
    }
    const dummyObject = await toolbox.provider.getObject(objectId);
    expect(dummyObject.details.data.fields.description).to.equal("Hello");
    expect(dummyObject.details.data.fields.number).to.equal('1');
  });

  it('Test accessing dynamic object field', async () => {
    const dummyDofInfo = await toolbox.provider.getDynamicFields(objectId);

    // make sure that only one dynamic object field is added
    expect(dummyDofInfo.data.length).to.equal(1);
    
    let dofName = dummyDofInfo.data[0].name;
    const dummyDof = await toolbox.provider.getDynamicFieldObject(objectId, dofName);

    expect(dummyDof.details.data.fields.description).to.equal("dummy dof");

  });
});



