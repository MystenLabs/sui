// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as mockttp from 'mockttp';

export const mockServer: mockttp.Mockttp = mockttp.getLocal();

export const MOCK_PORT = 9999;
export const MOCK_URL = 'http://127.0.0.1';
export const MOCK_ENDPOINT = `${MOCK_URL}:${MOCK_PORT}/`;

export const mockRpcResponse = async ({
  method,
  params,
  value,
  error,
}: {
  method: string;
  params: Array<any>;
  value?: any;
  error?: any;
}) => {
  await mockServer
    .forPost('/')
    .withJsonBodyIncluding({
      jsonrpc: '2.0',
      method,
      params,
    })
    .thenReply(
      200,
      JSON.stringify({
        jsonrpc: '2.0',
        id: '',
        error,
        result: value,
      })
    );
};
