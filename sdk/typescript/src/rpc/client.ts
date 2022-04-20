// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import RpcClient from 'jayson/lib/client/browser';
import {
  literal,
  type as pick,
  string,
  Struct,
  unknown,
  assert,
  optional,
  any,
  is,
} from 'superstruct';

/**
 * An object defining headers to be passed to the RPC server
 */
export type HttpHeaders = { [header: string]: string };

export class JsonRpcClient {
  private rpcClient: RpcClient;

  constructor(url: string, httpHeaders?: HttpHeaders) {
    this.rpcClient = this.createRpcClient(url, httpHeaders);
  }

  private createRpcClient(url: string, httpHeaders?: HttpHeaders): RpcClient {
    const client = new RpcClient(async (request, callback) => {
      const options = {
        method: 'POST',
        body: request,
        headers: Object.assign(
          {
            'Content-Type': 'application/json',
          },
          httpHeaders || {}
        ),
      };

      try {
        let res: Response = await fetch(url, options);
        const text = await res.text();
        if (res.ok) {
          callback(null, text);
        } else {
          callback(new Error(`${res.status} ${res.statusText}: ${text}`));
        }
      } catch (err) {
        if (err instanceof Error) callback(err);
      }
    }, {});

    return client;
  }

  async requestWithType<T, S>(
    method: string,
    args: Array<any>,
    schema: Struct<T, S>
  ): Promise<T> {
    const response = await this.request(method, args);
    if (is(response, ErrorResponse)) {
      throw new Error(`RPC Error: ${response.error.message}`);
    } else if (is(response, ValidResponse)) {
      assert(response.result, schema);
      return response.result;
    }
    throw new Error(`Unexpected RPC Response: ${response}`);
  }

  async request(method: string, args: Array<any>): Promise<any> {
    return new Promise((resolve, reject) => {
      this.rpcClient.request(method, args, (err: any, response: any) => {
        if (err) {
          reject(err);
          return;
        }
        resolve(response);
      });
    });
  }
}

const ValidResponse = pick({
  jsonrpc: literal('2.0'),
  id: string(),
  result: unknown(),
});

const ErrorResponse = pick({
  jsonrpc: literal('2.0'),
  id: string(),
  error: pick({
    code: unknown(),
    message: string(),
    data: optional(any()),
  }),
});
