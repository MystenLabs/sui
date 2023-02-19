// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import RpcClient from 'jayson/lib/client/browser/index.js';
import fetch from 'cross-fetch';
import {
  any,
  Infer,
  is,
  literal,
  object,
  optional,
  string,
  Struct,
} from 'superstruct';

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

const TYPE_MISMATCH_ERROR =
  `The response returned from RPC server does not match ` +
  `the TypeScript definition. This is likely because the SDK version is not ` +
  `compatible with the RPC server. Please update your SDK version to the latest. `;

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
    this.rpcClient = this.createRpcClient(url, httpHeaders);
  }

  private createRpcClient(url: string, httpHeaders?: HttpHeaders): RpcClient {
    const client = new RpcClient(
      async (
        request: any,
        callback: (arg0: Error | null, arg1?: string | undefined) => void,
      ) => {
        const options = {
          method: 'POST',
          body: request,
          headers: Object.assign(
            {
              'Content-Type': 'application/json',
            },
            httpHeaders || {},
          ),
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
          if (err instanceof Error) callback(err);
        }
      },
      {},
    );

    return client;
  }

  async requestWithType<T>(
    method: string,
    args: Array<any>,
    struct: Struct<T>,
    skipDataValidation: boolean = false,
  ): Promise<T> {
    const response = await this.request(method, args);
    if (is(response, ErrorResponse)) {
      throw new Error(`RPC Error: ${response.error.message}`);
    } else if (is(response, ValidResponse)) {
      // TODO: Improve error messaging here using superstruct asserts
      const expectedSchema = is(response.result, struct);
      const errMsg =
        TYPE_MISMATCH_ERROR +
        `Result received was: ${JSON.stringify(response.result)}`;

      if (skipDataValidation && !expectedSchema) {
        console.warn(errMsg);
        return response.result;
      } else if (!expectedSchema) {
        throw new Error(`RPC Error: ${errMsg}`);
      }
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

  // TODO: Improve validation errors:
  async batchRequestWithType<T>(
    requests: RpcParams[],
    struct: Struct<T>,
    skipDataValidation: boolean = false,
  ): Promise<T[]> {
    const responses = await this.batchRequest(requests);
    // TODO: supports other error modes such as throw or return
    const validResponses = responses.filter(
      (response: any) =>
        is(response, ValidResponse) &&
        (skipDataValidation || is(response.result, struct)),
    );

    if (responses.length > validResponses.length) {
      console.warn(
        `Batch request contains invalid responses. ${
          responses.length - validResponses.length
        } of the ${responses.length} requests has invalid schema.`,
      );
      const exampleTypeMismatch = responses.find(
        (r: any) => !is(r.result, struct),
      );
      const exampleInvalidResponseIndex = responses.findIndex(
        (r: any) => !is(r, ValidResponse),
      );
      if (exampleTypeMismatch) {
        console.warn(
          TYPE_MISMATCH_ERROR +
            `One example mismatch is: ${JSON.stringify(
              exampleTypeMismatch.result,
            )}`,
        );
      }
      if (exampleInvalidResponseIndex !== -1) {
        console.warn(
          `The request ${JSON.stringify(
            requests[exampleInvalidResponseIndex],
          )} within a batch request returns an invalid response ${JSON.stringify(
            responses[exampleInvalidResponseIndex],
          )}`,
        );
      }
    }

    return validResponses.map(
      (response: Infer<typeof ValidResponse>) => response.result,
    );
  }

  async batchRequest(requests: RpcParams[]): Promise<any> {
    return new Promise((resolve, reject) => {
      // Do nothing if requests is empty
      if (requests.length === 0) resolve([]);

      const batch = requests.map((params) => {
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
