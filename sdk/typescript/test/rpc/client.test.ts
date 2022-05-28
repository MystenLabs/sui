// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcClient } from '../../src/rpc/client';
import {
  mockRpcResponse,
  mockServer,
  MOCK_ENDPOINT,
  MOCK_PORT,
} from '../mocks/rpc-http';
import { isGetOwnedObjectsResponse } from '../../src/index.guard';
import { SuiObjectInfo } from '../../src';

const EXAMPLE_OBJECT: SuiObjectInfo = {
  objectId: '8dc6a6f70564e29a01c7293a9c03818fda2d049f',
  version: 0,
  digest: 'CI8Sf+t3Xrt5h9ENlmyR8bbMVfg6df3vSDc08Gbk9/g=',
  owner: {
    AddressOwner: '0x215592226abfec8d03fbbeb8b30eb0d2129c94b0',
  },
  type: 'moveObject',
  previousTransaction: '4RJfkN9SgLYdb0LqxBHh6lfRPicQ8FLJgzi9w2COcTo=',
};

describe('JSON-RPC Client', () => {
  const server = mockServer;
  let client: JsonRpcClient;

  beforeEach(() => {
    server.start(MOCK_PORT);
    client = new JsonRpcClient(MOCK_ENDPOINT);
  });

  afterEach(() => {
    server.stop();
  });

  it('requestWithType', async () => {
    await mockRpcResponse({
      method: 'sui_getOwnedObjectsByAddress',
      params: [],
      value: [EXAMPLE_OBJECT],
    });

    const resp = await client.requestWithType(
      'sui_getOwnedObjectsByAddress',
      [],
      isGetOwnedObjectsResponse
    );
    expect(resp.length).toEqual(1);
    expect(resp[0]).toEqual(EXAMPLE_OBJECT);
  });
});
