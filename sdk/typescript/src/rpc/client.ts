// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Client, HTTPTransport, RequestManager } from '@open-rpc/client-js';
import type { Struct } from 'superstruct';
import { validate } from 'superstruct';

import { PACKAGE_VERSION, TARGETED_RPC_VERSION } from '../version.js';
import { RPCValidationError } from './errors.js';

/**
 * An object defining headers to be passed to the RPC server
 */
export type HttpHeaders = { [header: string]: string };

export class JsonRpcClient {
	private rpcClient: Client;

	constructor(url: string, httpHeaders?: HttpHeaders) {
		const transport = new HTTPTransport(url, {
			headers: {
				'Content-Type': 'application/json',
				'Client-Sdk-Type': 'typescript',
				'Client-Sdk-Version': PACKAGE_VERSION,
				'Client-Target-Api-Version': TARGETED_RPC_VERSION,
				...httpHeaders,
			},
		});

		this.rpcClient = new Client(new RequestManager([transport]));
	}

	async requestWithType<T>(method: string, args: any[], struct: Struct<T>): Promise<T> {
		const req = { method, args };

		const response = await this.request(method, args);

		if (process.env.NODE_ENV === 'test') {
			const [err] = validate(response, struct);
			if (err) {
				throw new RPCValidationError({
					req,
					result: response,
					cause: err,
				});
			}
		}

		return response;
	}

	async request(method: string, params: any[]): Promise<any> {
		return await this.rpcClient.request({ method, params });
	}
}
