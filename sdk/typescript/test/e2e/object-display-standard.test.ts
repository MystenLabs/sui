// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { ObjectId, RawSigner, getObjectDisplay } from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test Object Display Standard', () => {
  let toolbox: TestToolbox;
  let signer: RawSigner;
  let packageId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    signer = new RawSigner(toolbox.keypair, toolbox.provider);
    const packagePath = __dirname + '/./data/display_test';
    packageId = await publishPackage(signer, packagePath);
  });

  it('Test getting Display fields', async () => {
    const boarId = (
      await toolbox.provider.getObjectsOwnedByAddress(
        toolbox.address(),
        `${packageId}::boars::Boar`,
      )
    )[0].objectId;
    const display = getObjectDisplay(
      await toolbox.provider.getObject(boarId, { showDisplay: true }),
    );
    expect(display).toEqual({
      age: '10',
      buyer: `0x${toolbox.address()}`,
      creator: 'Chris',
      description: `Unique Boar from the Boars collection with First Boar and ${boarId}`,
      img_url: 'https://get-a-boar.com/first.png',
      name: 'First Boar',
      price: '',
      project_url: 'https://get-a-boar.com/',
      full_url: 'https://get-a-boar.fullurl.com/',
      escape_syntax: '{name}',
    });
  });

  it('Test getting Display fields for object that has no display object', async () => {
    const coinId = (
      await toolbox.provider.getGasObjectsOwnedByAddress(toolbox.address())
    )[0].objectId;
    const display = getObjectDisplay(
      await toolbox.provider.getObject(coinId, { showDisplay: true }),
    );
    expect(display).toEqual(undefined);
  });
});
