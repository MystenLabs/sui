import { describe, it, expect, beforeAll } from 'vitest';
import { publishPackage, setup, TestToolbox } from './utils/setup';
import { RawSigner, ObjectId, LocalTxnDataSerializer } from '../../src';

describe('Test Struct Metadata', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;
  let objectId: string;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );

    const packagePath = __dirname + '/./data/struct_metadata';
    packageId = await publishPackage(signer, true, packagePath);
  });

  it('Test accessing struct metadata', async () => {

    let address = await signer.getAddress();

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

});

