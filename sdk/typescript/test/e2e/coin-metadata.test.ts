// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import {
  getEvents,
  getObjectExistsResponse,
  getObjectFields,
  LocalTxnDataSerializer,
  ObjectId,
  RawSigner,
} from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test Coin Metadata', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
    const packagePath = __dirname + '/./data/coin_metadata';
    packageId = await publishPackage(signer, true, packagePath);
  });

  it('Test accessing coin metadata', async () => {
    // TODO: add a new RPC endpoint for fetching coin metadata
    const objectResponse = await toolbox.provider.getObject(packageId);
    const publishTxnDigest =
      getObjectExistsResponse(objectResponse)!.previousTransaction;
    const publishTxn = await toolbox.provider.getTransactionWithEffects(
      publishTxnDigest
    );
    const coinMetadataId = getEvents(publishTxn)!
      .map((event) => {
        if (
          'newObject' in event &&
          event.newObject.objectType.includes('CoinMetadata')
        ) {
          return event.newObject.objectId;
        }
        return undefined;
      })
      .filter((e) => e)[0]!;
    const coinMetadata = getObjectFields(
      await toolbox.provider.getObject(coinMetadataId)
    )!;
    expect(coinMetadata.decimals).to.equal(2);
    expect(coinMetadata.name).to.equal('Test Coin');
    expect(coinMetadata.description).to.equal('Test coin metadata');
    expect(coinMetadata.icon_url).to.equal('http://sui.io');
  });
});
