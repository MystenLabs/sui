// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { TypedDocumentNode } from '@graphql-typed-document-node/core';
import type {
	SuiTransport,
	SuiTransportRequestOptions,
	SuiTransportSubscribeOptions,
} from '@mysten/sui.js/client';
import type { DocumentNode } from 'graphql';
import { print } from 'graphql';

import { TypedDocumentString } from './generated/queries.js';
import { RPC_METHODS, unsupportedMethod } from './methods.js';

export interface SuiClientGraphQLTransportOptions {
	url: string;
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

	constructor(options: SuiClientGraphQLTransportOptions) {
		this.#options = options;
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

		if (!method) {
			unsupportedMethod(input.method);
		}

		return method(this, input.params as never) as Promise<T>;
	}

	async subscribe<T = unknown>(
		input: SuiTransportSubscribeOptions<T>,
	): Promise<() => Promise<boolean>> {
		unsupportedMethod(input.method);
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
