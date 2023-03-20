// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RequestParamsLike } from 'jayson';

interface RPCErrorRequest {
  method: string;
  args: RequestParamsLike;
}

export class RPCError extends Error {
  req: RPCErrorRequest;
  code?: unknown;
  data?: unknown;

  constructor(options: {
    req: RPCErrorRequest;
    code?: unknown;
    data?: unknown;
    cause?: Error;
  }) {
    super('RPC Error', { cause: options.cause });

    this.req = options.req;
    this.code = options.code;
    this.data = options.data;
  }
}

export class RPCValidationError extends Error {
  req: RPCErrorRequest;
  result?: unknown;

  constructor(options: {
    req: RPCErrorRequest;
    result?: unknown;
    cause?: Error;
  }) {
    super(
      'RPC Validation Error: The response returned from RPC server does not match the TypeScript definition. This is likely because the SDK version is not compatible with the RPC server.',
      { cause: options.cause },
    );

    this.req = options.req;
    this.result = options.result;
  }
}

export class FaucetRateLimitError extends Error {}
