// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type as pick, array } from 'superstruct';
import { JsonRpcClient } from '../../src/rpc/client';
import { ObjectRef } from '../../src';
import {
  mockRpcResponse,
  mockServer,
  MOCK_ENDPOINT,
  MOCK_PORT,
} from '../mocks/rpc-http';

const EXAMPLE_OBJECT = {
  objectId: '8dc6a6f70564e29a01c7293a9c03818fda2d049f',
  version: 0,
  digest: 'CI8Sf+t3Xrt5h9ENlmyR8bbMVfg6df3vSDc08Gbk9/g=',
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
      method: 'sui_getOwnedObjects',
      params: [],
      value: {
        objects: [EXAMPLE_OBJECT],
      },
    });

    const resp = await client.requestWithType(
      'sui_getOwnedObjects',
      [],
      pick({ objects: array(ObjectRef) })
    );
    expect(resp.objects.length).toEqual(1);
    expect(resp.objects[0]).toEqual(EXAMPLE_OBJECT);
  });
});
