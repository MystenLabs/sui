// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import RpcClient from 'jayson/lib/client/browser/index.js';
import {
  any,
  is,
  literal,
  object,
  optional,
  string,
  Struct,
  validate,
} from 'superstruct';
import { pkgVersion } from '../pkg-version';
import { TARGETED_RPC_VERSION } from '../providers/json-rpc-provider';
import { RequestParamsLike } from 'jayson';
import { RPCError, RPCValidationError } from '../utils/errors';

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

export const ValidResponse = object({
  jsonrpc: literal('2.0'),
  id: string(),
  result: any(),
});

export const ErrorResponse = object({
  jsonrpc: literal('2.0'),
  id: string(),
  error: object({
    code: any(),
    message: string(),
    data: optional(any()),
  }),
});

export class JsonRpcClient {
  private rpcClient: RpcClient;

  constructor(url: string, httpHeaders?: HttpHeaders) {
    this.rpcClient = new RpcClient(
      async (
        request: any,
        callback: (arg0: Error | null, arg1?: string | undefined) => void,
      ) => {
        const options = {
          method: 'POST',
          body: request,
          headers: {
            'Content-Type': 'application/json',
            'Client-Sdk-Type': 'typescript',
            'Client-Sdk-Version': pkgVersion,
            'Client-Target-Api-Version': TARGETED_RPC_VERSION,
            ...httpHeaders,
          },
        };

        try {
          let res: Response = await fetch(url, options);
          const result = await res.text();
          if (res.ok) {
            callback(null, result);
          } else {
            const isHtml = res.headers.get('content-type') === 'text/html';
            callback(
              new Error(
                `${res.status} ${res.statusText}${isHtml ? '' : `: ${result}`}`,
              ),
            );
          }
        } catch (err) {
          callback(err as Error);
        }
      },
      {},
    );
  }

  async requestWithType<T>(
    method: string,
    args: RequestParamsLike,
    struct: Struct<T>,
    skipDataValidation: boolean = false,
  ): Promise<T> {
    const req = { method, args };

    const response = await this.request(method, args);
    if (is(response, ErrorResponse)) {
      throw new RPCError({
        req,
        code: response.error.code,
        data: response.error.data,
        cause: new Error(response.error.message),
      });
    } else if (is(response, ValidResponse)) {
      const [err] = validate(response.result, struct);

      if (skipDataValidation && err) {
        console.warn(
          new RPCValidationError({
            req,
            result: response.result,
            cause: err,
          }),
        );
        return response.result;
      } else if (err) {
        throw new RPCValidationError({
          req,
          result: response.result,
          cause: err,
        });
      }
      return response.result;
    }
    // Unexpected response:
    throw new RPCError({ req, data: response });
  }

  async request(method: string, args: RequestParamsLike): Promise<any> {
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
