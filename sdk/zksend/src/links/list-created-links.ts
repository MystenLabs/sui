// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from '@mysten/sui.js/bcs';
import type { SuiClient } from '@mysten/sui.js/client';
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';
import { fromB64, normalizeSuiAddress } from '@mysten/sui.js/utils';

import { ZkSendLink } from './claim.js';
import type { ZkBagContractOptions } from './zk-bag.js';
import { MAINNET_CONTRACT_IDS } from './zk-bag.js';

const ListCreatedLinksQuery = graphql(`
	query listCreatedLinks($address: SuiAddress!, $function: String!, $cursor: String) {
		transactionBlocks(
			last: 10
			before: $cursor
			filter: { signAddress: $address, function: $function, kind: PROGRAMMABLE_TX }
		) {
			pageInfo {
				startCursor
				hasPreviousPage
			}
			nodes {
				effects {
					timestamp
				}
				digest
				kind {
					__typename
					... on ProgrammableTransactionBlock {
						inputs(first: 10) {
							nodes {
								__typename
								... on Pure {
									bytes
								}
							}
						}
						transactions(first: 10) {
							nodes {
								__typename
								... on MoveCallTransaction {
									module
									functionName
									package
									arguments {
										__typename
										... on Input {
											ix
										}
									}
								}
							}
						}
					}
				}
			}
		}
	}
`);

export async function listCreatedLinks({
	address,
	cursor,
	network,
	contract = MAINNET_CONTRACT_IDS,
	fetch: fetchFn,
	...linkOptions
}: {
	address: string;
	contract?: ZkBagContractOptions;
	cursor?: string;
	network?: 'mainnet' | 'testnet';

	// Link options:
	host?: string;
	path?: string;
	client?: SuiClient;
	fetch?: typeof fetch;
}) {
	const gqlClient = new SuiGraphQLClient({
		url:
			network === 'testnet'
				? 'https://sui-testnet.mystenlabs.com/graphql'
				: 'https://sui-mainnet.mystenlabs.com/graphql',
		fetch: fetchFn,
	});

	const packageId = normalizeSuiAddress(contract.packageId);

	const page = await gqlClient.query({
		query: ListCreatedLinksQuery,
		variables: {
			address,
			cursor,
			function: `${packageId}::zk_bag::new`,
		},
	});

	const transactionBlocks = page.data?.transactionBlocks;

	if (!transactionBlocks || page.errors?.length) {
		throw new Error('Failed to load created links');
	}

	const links = (
		await Promise.all(
			transactionBlocks.nodes.map(async (node) => {
				if (node.kind?.__typename !== 'ProgrammableTransactionBlock') {
					throw new Error('Invalid transaction block');
				}

				const fn = node.kind.transactions.nodes.find(
					(fn) =>
						fn.__typename === 'MoveCallTransaction' &&
						fn.package === packageId &&
						fn.module === 'zk_bag' &&
						fn.functionName === 'new',
				);

				if (fn?.__typename !== 'MoveCallTransaction') {
					return null;
				}

				const addressArg = fn.arguments[1];

				if (addressArg.__typename !== 'Input') {
					throw new Error('Invalid address argument');
				}

				const input = node.kind.inputs.nodes[addressArg.ix];

				if (input.__typename !== 'Pure') {
					throw new Error('Expected Address input to be a Pure value');
				}

				const address = bcs.Address.parse(fromB64(input.bytes as string));

				const link = new ZkSendLink({
					network,
					address,
					contract,
					isContractLink: true,
					...linkOptions,
				});

				await link.loadAssets();

				return {
					link,
					claimed: !!link.claimed,
					assets: link.assets!,
					digest: node.digest,
					createdAt: node.effects?.timestamp!,
				};
			}),
		)
	).reverse();

	return {
		cursor: transactionBlocks.pageInfo.startCursor,
		hasNextPage: transactionBlocks.pageInfo.hasPreviousPage,
		links: links.filter((link): link is NonNullable<typeof link> => link !== null),
	};
}
