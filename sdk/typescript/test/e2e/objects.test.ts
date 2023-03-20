// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, it, expect, beforeAll } from 'vitest';
import { getObjectType, SuiObjectData } from '../../src';
import { setup, TestToolbox } from './utils/setup';

describe('Object Reading API', () => {
  let toolbox: TestToolbox;

  beforeAll(async () => {
    toolbox = await setup();
  });

  it('Get Owned Objects', async () => {
    const gasObjects = await toolbox.provider.getOwnedObjects({
      owner: toolbox.address(),
    });
    expect(gasObjects.data.length).to.greaterThan(0);
  });

  it('Get Object', async () => {
    const gasObjects = await toolbox.getGasObjectsOwnedByAddress();
    expect(gasObjects.length).to.greaterThan(0);
    const objectInfos = await Promise.all(
      gasObjects.map((gasObject) => {
        const details = gasObject.details as SuiObjectData;
        return toolbox.provider.getObject({
          id: details.objectId,
          options: { showType: true },
        });
      }),
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
    const gasObjectIds = gasObjects.map((gasObject) => {
      const details = gasObject.details as SuiObjectData;
      return details.objectId;
    });
    const objectInfos = await toolbox.provider.multiGetObjects({
      ids: gasObjectIds,
      options: {
        showType: true,
      },
    });

    expect(gasObjects.length).to.equal(objectInfos.length);

    objectInfos.forEach((objectInfo) =>
      expect(getObjectType(objectInfo)).to.equal(
        '0x2::coin::Coin<0x2::sui::SUI>',
      ),
    );
  });
});
