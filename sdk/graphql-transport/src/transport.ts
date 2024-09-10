// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TypedDocumentNode } from '@graphql-typed-document-node/core';
import type {
	SuiTransport,
	SuiTransportRequestOptions,
	SuiTransportSubscribeOptions,
} from '@mysten/sui/client';
import { SuiHTTPTransport } from '@mysten/sui/client';
import type { DocumentNode } from 'graphql';
import { print } from 'graphql';

import { TypedDocumentString } from './generated/queries.js';
import { RPC_METHODS, UnsupportedMethodError, UnsupportedParamError } from './methods.js';

export interface SuiClientGraphQLTransportOptions {
	url: string;
	fallbackFullNodeUrl?: string;
	fallbackMethods?: (keyof typeof RPC_METHODS)[];
}

export type GraphQLDocument<
	Result = Record<string, unknown>,
	Variables = Record<string, unknown>,
> =
	| string
	| DocumentNode
	| TypedDocumentNode<Result, Variables>
	| TypedDocumentString<Result, Variables>;

export type GraphQLQueryOptions<
	Result = Record<string, unknown>,
	Variables = Record<string, unknown>,
> = {
	query: GraphQLDocument<Result, Variables>;
	operationName?: string;
	extensions?: Record<string, unknown>;
} & (Variables extends { [key: string]: never }
	? { variables?: Variables }
	: {
			variables: Variables;
		});

export type GraphQLQueryResult<Result = Record<string, unknown>> = {
	data?: Result;
	errors?: GraphQLResponseErrors;
	extensions?: Record<string, unknown>;
};

export type GraphQLResponseErrors = Array<{
	message: string;
	locations?: { line: number; column: number }[];
	path?: (string | number)[];
}>;

export class SuiClientGraphQLTransport implements SuiTransport {
	#options: SuiClientGraphQLTransportOptions;
	#fallbackTransport?: SuiTransport;
	#fallbackMethods: (keyof typeof RPC_METHODS)[];

	constructor(options: SuiClientGraphQLTransportOptions) {
		this.#options = options;
		this.#fallbackMethods = options.fallbackMethods || [
			'executeTransactionBlock',
			'dryRunTransactionBlock',
			'devInspectTransactionBlock',
		];

		if (options.fallbackFullNodeUrl) {
			this.#fallbackTransport = new SuiHTTPTransport({
				url: options.fallbackFullNodeUrl,
			});
		}
	}

	async graphqlQuery<
		Result = Record<string, unknown>,
		Variables = Record<string, unknown>,
		Data = Result,
	>(
		options: GraphQLQueryOptions<Result, Variables>,
		getData?: (result: Result) => Data,
	): Promise<NonNullable<Data>> {
		const res = await this.graphqlRequest(options);

		if (!res.ok) {
			throw new Error('Failed to fetch');
		}

		const { data, errors } = (await res.json()) as GraphQLQueryResult<Result>;

		handleGraphQLErrors(errors);

		const extractedData = data && (getData ? getData(data) : data);

		if (extractedData == null) {
			throw new Error('Missing response data');
		}

		return extractedData as NonNullable<Data>;
	}

	async graphqlRequest<Result = Record<string, unknown>, Variables = Record<string, unknown>>(
		options: GraphQLQueryOptions<Result, Variables>,
	): Promise<Response> {
		return fetch(this.#options.url, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({
				query:
					typeof options.query === 'string' || options.query instanceof TypedDocumentString
						? options.query.toString()
						: print(options.query),
				variables: options.variables,
				extensions: options.extensions,
				operationName: options.operationName,
			}),
		});
	}

	async request<T = unknown>(input: SuiTransportRequestOptions): Promise<T> {
		let clientMethod: keyof typeof RPC_METHODS;

		switch (input.method) {
			case 'rpc.discover':
				clientMethod = 'getRpcApiVersion';
				break;
			case 'suix_getLatestAddressMetrics':
				clientMethod = 'getAddressMetrics';
				break;
			default:
				clientMethod = input.method.split('_')[1] as keyof typeof RPC_METHODS;
		}

		const method = RPC_METHODS[clientMethod];

		if (!method || this.#fallbackMethods.includes(clientMethod)) {
			return this.#unsupportedMethod(input);
		}

		try {
			return method(this, input.params as never) as Promise<T>;
		} catch (error) {
			if (this.#fallbackTransport && error instanceof UnsupportedParamError) {
				return this.#fallbackTransport.request(input);
			}

			throw error;
		}
	}

	async subscribe<T = unknown>(
		input: SuiTransportSubscribeOptions<T>,
	): Promise<() => Promise<boolean>> {
		if (!this.#fallbackTransport) {
			throw new UnsupportedMethodError(input.method);
		}

		return this.#fallbackTransport.subscribe(input);
	}

	async #unsupportedMethod<T = unknown>(input: SuiTransportRequestOptions): Promise<T> {
		if (!this.#fallbackTransport) {
			throw new UnsupportedMethodError(input.method);
		}

		return this.#fallbackTransport.request(input);
	}
}

function handleGraphQLErrors(errors: GraphQLResponseErrors | undefined): void {
	if (!errors || errors.length === 0) return;

	const errorInstances = errors.map((error) => new GraphQLResponseError(error));

	if (errorInstances.length === 1) {
		throw errorInstances[0];
	}

	throw new AggregateError(errorInstances);
}

class GraphQLResponseError extends Error {
	locations?: Array<{ line: number; column: number }>;

	constructor(error: GraphQLResponseErrors[0]) {
		super(error.message);
		this.locations = error.locations;
	}
}
