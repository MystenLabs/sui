// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { ObjectId, getObjectDisplay, SuiObjectData } from '../../src';
import { publishPackage, setup, TestToolbox } from './utils/setup';

describe('Test Object Display Standard', () => {
  let toolbox: TestToolbox;
  let packageId: ObjectId;

  beforeAll(async () => {
    toolbox = await setup();
    const packagePath = __dirname + '/./data/display_test';
    ({ packageId } = await publishPackage(packagePath, toolbox));
  });

  it('Test getting Display fields', async () => {
    const resp = (
      await toolbox.provider.getOwnedObjects({
        owner: toolbox.address(),
        options: { showDisplay: true, showType: true },
        filter: { StructType: `${packageId}::boars::Boar` },
      })
    ).data;
    const data = resp[0].details as SuiObjectData;
    const boarId = data.objectId;
    const display = getObjectDisplay(
      await toolbox.provider.getObject({
        id: boarId,
        options: { showDisplay: true },
      }),
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
    const coin = (await toolbox.getGasObjectsOwnedByAddress())[0]
      .details as SuiObjectData;
    const coinId = coin.objectId;
    const display = getObjectDisplay(
      await toolbox.provider.getObject({
        id: coinId,
        options: { showDisplay: true },
      }),
    );
    expect(display).toEqual(undefined);
  });
});
