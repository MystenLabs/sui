// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getObjectType } from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Object Reading API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Get Owned Objects', async () => {
    const gasObjects = await toolbox.provider.getObjectsOwnedByAddress(
      toolbox.address(),
    );
    expect(gasObjects.length).to.greaterThan(0);
  });

  it('Get Object', async () => {
    const gasObjects = await toolbox.getGasObjectsOwnedByAddress();
    expect(gasObjects.length).to.greaterThan(0);
    const objectInfos = await Promise.all(
      gasObjects.map((gasObject) =>
        toolbox.provider.getObject(gasObject['objectId'], { showType: true }),
      ),
    );
    objectInfos.forEach((objectInfo) =>
      expect(getObjectType(objectInfo)).to.equal(
        '0x2::coin::Coin<0x2::sui::SUI>',
      ),
    );
  });

  it('Get Objects', async () => {
    const gasObjects = await toolbox.getGasObjectsOwnedByAddress();
    expect(gasObjects.length).to.greaterThan(0);
    const gasObjectIds = gasObjects.map((gasObject) => gasObject['objectId']);
    const objectInfos = await toolbox.provider.getObjectBatch(gasObjectIds, {
      showType: true,
    });

    expect(gasObjects.length).to.equal(objectInfos.length);

    objectInfos.forEach((objectInfo) =>
      expect(getObjectType(objectInfo)).to.equal(
        '0x2::coin::Coin<0x2::sui::SUI>',
      ),
    );
  });
});
