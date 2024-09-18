// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiGraphQLClient } from '../../graphql/client.js';
import type { BuildTransactionOptions } from '../json-rpc-resolver.js';
import type { TransactionDataBuilder } from '../TransactionData.js';
import type { NamedPackagesPluginCache, NameResolutionRequest } from './utils.js';
import { findTransactionBlockNames, listToRequests, replaceNames } from './utils.js';

export type NamedPackagesPluginOptions = {
	/**
	 * The SuiGraphQLClient to use for resolving names.
	 * The endpoint should be the GraphQL endpoint of the network you are targeting.
	 * For non-mainnet networks, if the plugin doesn't work as expected, you need to validate that the
	 * RPC provider has support for the `packageByName` and `typeByName` queries (using external resolver).
	 */
	suiGraphQLClient: SuiGraphQLClient;
	/**
	 * The number of names to resolve in each batch request.
	 * Needs to be calculated based on the GraphQL query limits.
	 */
	pageSize?: number;
	/**
	 * Local overrides for the resolution plugin. Pass this to pre-populate
	 * the cache with known packages / types (especially useful for local or CI testing).
	 *
	 * 	Expected format example:
	 *  {
	 * 		packages: {
	 * 			'@framework/std': '0x1234',
	 * 		},
	 * 		types: {
	 * 			'@framework/std::string::String': '0x1234::string::String',
	 * 		},
	 * 	}
	 *
	 */
	overrides?: NamedPackagesPluginCache;
};

/**
 * @experimental This plugin is in experimental phase and there might be breaking changes in the future
 *
 * Adds named resolution so that you can use .move names in your transactions.
 * e.g. `@org/app::type::Type` will be resolved to `0x1234::type::Type`.
 * This plugin will resolve all names & types in the transaction block.
 *
 * To install this plugin globally in your app, use:
 * ```
 * Transaction.registerGlobalSerializationPlugin("namedPackagesPlugin", namedPackagesPlugin({ suiGraphQLClient }));
 * ```
 *
 * You can also define `overrides` to pre-populate name resolutions locally (removes the GraphQL request).
 */
export const namedPackagesPlugin = ({
	suiGraphQLClient,
	pageSize = 10,
	overrides = { packages: {}, types: {} },
}: NamedPackagesPluginOptions) => {
	const cache = {
		packages: { ...overrides.packages },
		types: { ...overrides.types },
	};

	return async (
		transactionData: TransactionDataBuilder,
		_buildOptions: BuildTransactionOptions,
		next: () => Promise<void>,
	) => {
		const names = findTransactionBlockNames(transactionData);
		const batches = listToRequests(
			{
				packages: names.packages.filter((x) => !cache.packages[x]),
				types: names.types.filter((x) => !cache.types[x]),
			},
			pageSize,
		);

		// now we need to bulk resolve all the names + types, and replace them in the transaction data.
		(await Promise.all(batches.map((batch) => query(suiGraphQLClient, batch)))).forEach((res) => {
			Object.assign(cache.types, res.types);
			Object.assign(cache.packages, res.packages);
		});

		replaceNames(transactionData, cache);

		await next();
	};

	async function query(client: SuiGraphQLClient, requests: NameResolutionRequest[]) {
		const results: NamedPackagesPluginCache = { packages: {}, types: {} };
		// avoid making a request if there are no names to resolve.
		if (requests.length === 0) return results;

		// Create multiple queries for each name / type we need to resolve
		// TODO: Replace with bulk APIs when available.
		const gqlQuery = `{
        ${requests.map((req) => {
					const request = req.type === 'package' ? 'packageByName' : 'typeByName';
					const fields = req.type === 'package' ? 'address' : 'repr';

					return `${gqlQueryKey(req.id)}: ${request}(name:"${req.name}") {
                    ${fields}
                }`;
				})}
    }`;

		const result = await client.query({
			query: gqlQuery,
			variables: undefined,
		});

		if (result.errors) throw new Error(JSON.stringify({ query: gqlQuery, errors: result.errors }));

		// Parse the results and create a map of `<name|type> -> <address|repr>`
		for (const req of requests) {
			const key = gqlQueryKey(req.id);
			if (!result.data || !result.data[key]) throw new Error(`No result found for: ${req.name}`);
			const data = result.data[key] as { address?: string; repr?: string };

			if (req.type === 'package') results.packages[req.name] = data.address!;
			if (req.type === 'moveType') results.types[req.name] = data.repr!;
		}

		return results;
	}
};

const gqlQueryKey = (idx: number) => `key_${idx}`;
