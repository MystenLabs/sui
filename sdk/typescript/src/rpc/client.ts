// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import RpcClient from 'jayson/lib/client/browser';
import fetch from 'cross-fetch';
import { isErrorResponse, isValidResponse } from './client.guard';
import * as LosslessJSON from 'lossless-json';

/**
 * An object defining headers to be passed to the RPC server
 */
export type HttpHeaders = { [header: string]: string };

/**
 * @internal
 */
export type RpcParams = {
  method: string;
  args: Array<any>;
};

const httpRegex = new RegExp('^http');
const portRegex = new RegExp(':[0-9]{1,5}$');
export const getWebsocketUrl = (httpUrl: string): string => {
  console.log('trying to convert http url to ws', httpUrl);
  let wsUrl = httpUrl.replace(httpRegex, 'ws');
  wsUrl = wsUrl.replace(portRegex, '');
  wsUrl = `${wsUrl}:9001`;

  console.log('wsUrl', wsUrl);
  return wsUrl;
};

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
          const result = JSON.stringify(
            LosslessJSON.parse(text, (key, value) => {
              if (value == null) {
                return value;
              }

              // TODO: This is a bad hack, we really shouldn't be doing this here:
              if (key === 'balance' && typeof value === 'number') {
                return value.toString();
              }

              try {
                if (value.isLosslessNumber) return value.valueOf();
              } catch {
                return value.toString();
              }
              return value;
            })
          );
          if (res.ok) {
            callback(null, result);
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

  async wsRequestWithType<T>(
    method: string,
    args: Array<any>,
    isT: (val: any) => val is T
  ): Promise<T> {
    const response = await this.wsRequest(method, args);
    if (isErrorResponse(response)) {
      throw new Error(`RPC Error: ${response.error.message}`);
    } else if (isValidResponse(response)) {
      if (isT(response.result)) return response.result;
        throw new Error(
          `RPC Error: result not of expected type. Result received was: ${JSON.stringify(
            response.result
          )}`
        );
    }
    throw new Error(`Unexpected Websocket RPC Response: ${response}`);
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
        throw new Error(
          `RPC Error: result not of expected type. Result received was: ${JSON.stringify(
            response.result
          )}`
        );
    }
    throw new Error(`Unexpected RPC Response: ${response}`);
  }

  async requestCallbackWithType<TResult, TCallbackResult>(
    method: string,
    args: Array<any>,
    _callback: (data: TCallbackResult) => any,
    isResultT: (val: any) => val is TResult,
    _isCallbackT: (val: any) => val is TCallbackResult
  ): Promise<TResult> {
    const response = await this.request(method, args);
    if (isErrorResponse(response)) {
      throw new Error(`RPC Error: ${response.error.message}`);
    } else if (isValidResponse(response)) {
      if (isResultT(response.result)) return response.result;
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

  async wsRequest(method: string, args: Array<any>): Promise<any> {
    return new Promise((resolve, reject) => {
      this.wsClient.request(method, args, (err: any, response: any) => {
        if (err) {
          reject(err);
          return;
        }
        resolve(response);
      });
    });
  }

  async batchRequestWithType<T>(
    requests: RpcParams[],
    isT: (val: any) => val is T
  ): Promise<T[]> {
    const responses = await this.batchRequest(requests);
    // TODO: supports other error modes such as throw or return
    const validResponses = responses.filter(
      (response: any) => isValidResponse(response) && isT(response.result)
    );

    return validResponses.map((response: ValidResponse) => response.result);
  }

  async batchRequest(requests: RpcParams[]): Promise<any> {
    return new Promise((resolve, reject) => {
      // Do nothing if requests is empty
      if (requests.length === 0) resolve([]);

      const batch = requests.map(params => {
        return this.rpcClient.request(params.method, params.args);
      });

      this.rpcClient.request(batch, (err: any, response: any) => {
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

export class JsonRpcWebsocketClient {

}