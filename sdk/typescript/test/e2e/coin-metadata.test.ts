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
  let shouldSkip: boolean;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(
      toolbox.keypair,
      toolbox.provider,
      new LocalTxnDataSerializer(toolbox.provider)
    );
    // TODO: This API is only available under version 0.17.0. Clean
    // up this once 0.17. is released
    const version = await toolbox.provider.getRpcApiVersion();
    if (version?.major === 0 && version?.minor < 17) {
      shouldSkip = true;
      return;
    }
    const packagePath = __dirname + '/./data/coin_metadata';
    packageId = await publishPackage(signer, true, packagePath);
  });

  it('Test accessing coin metadata', async () => {
    if (shouldSkip) {
      return;
    }
    const coinMetadata = await signer.provider.getCoinMetadata(
      `${packageId}::test::TEST`
    );
    expect(coinMetadata.decimals).to.equal(2);
    expect(coinMetadata.name).to.equal('Test Coin');
    expect(coinMetadata.description).to.equal('Test coin metadata');
    expect(coinMetadata.iconUrl).to.equal('http://sui.io');
  });
});
