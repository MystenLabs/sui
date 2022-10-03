// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getMoveObjectType } from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Object Reading API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Get Owned Objects', async () => {
    const gasObjects = await toolbox.provider.getObjectsOwnedByAddress(
      toolbox.address()
    );
    expect(gasObjects.length).to.greaterThan(0);
  });

  it('Get Object', async () => {
    const gasObjects = await toolbox.provider.getGasObjectsOwnedByAddress(
      toolbox.address()
    );
    expect(gasObjects.length).to.greaterThan(0);
    const objectInfos = await Promise.all(
      gasObjects.map((gasObject) =>
        toolbox.provider.getObject(gasObject['objectId'])
      )
    );
    objectInfos.forEach((objectInfo) =>
      expect(getMoveObjectType(objectInfo)).to.equal(
        '0x2::coin::Coin<0x2::sui::SUI>'
      )
    );
  });
});
