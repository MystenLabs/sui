// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import RpcClient from 'jayson/lib/client/browser';
import fetch from 'cross-fetch';
import { isErrorResponse, isValidResponse } from './client.guard';

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
    const client = new RpcClient(
      async (
        request: any,
        callback: (arg0: Error | null, arg1?: string | undefined) => void
      ) => {
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
      },
      {}
    );

    return client;
  }

  async requestWithType<T>(
    method: string,
    args: Array<any>,
    isT: (val: any) => val is T
  ): Promise<T> {
    const response = await this.request(method, args);
    if (isErrorResponse(response)) {
      throw new Error(`RPC Error: ${response.error.message}`);
    } else if (isValidResponse(response)) {
      if (isT(response.result)) return response.result;
      else
        throw new Error(
          `RPC Error: result not of expected type. Result received was: ${JSON.stringify(
            response.result
          )}`
        );
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

export type ValidResponse = {
  jsonrpc: '2.0';
  id: string;
  result: any;
};

export type ErrorResponse = {
  jsonrpc: '2.0';
  id: string;
  error: {
    code: any;
    message: string;
    data?: any;
  };
};
