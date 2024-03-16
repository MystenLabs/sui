// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@mysten/sui.js/graphql/schemas/2024-01';

import { bcs } from '@mysten/sui.js/bcs';
import type { SuiClient } from '@mysten/sui.js/client';
import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';
import type { TransactionBlock } from '@mysten/sui.js/transactions';
import { fromB64, normalizeSuiAddress } from '@mysten/sui.js/utils';

import { ZkSendLink } from './claim.js';
import type { ZkBagContractOptions } from './zk-bag.js';

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
	contract,
	...linkOptions
}: {
	address: string;
	contract: ZkBagContractOptions;
	cursor?: string;
	network?: 'mainnet' | 'testnet';
	// Link options:
	host?: string;
	path?: string;
	client?: SuiClient;
}) {
	const gqlClient = new SuiGraphQLClient({
		url:
			network === 'testnet'
				? 'https://sui-testnet.mystenlabs.com/graphql'
				: 'https://sui-mainnet.mystenlabs.com/graphql',
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

	const links = transactionBlocks.nodes
		.map((node) => {
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

			return new ZkSendLink({
				network,
				address,
				contract,
				isContractLink: true,
				...linkOptions,
			});
		})
		.reverse()
		.filter(Boolean) as ZkSendLink[];

	await Promise.all(links.map((link) => link.loadOwnedData()));

	return {
		cursor: transactionBlocks.pageInfo.startCursor,
		hasNextPage: transactionBlocks.pageInfo.hasPreviousPage,
		links,
	};
}

export function isClaimTransaction(
	txb: TransactionBlock,
	options: {
		packageId: string;
	},
) {
	let transfers = 0;

	for (const tx of txb.blockData.transactions) {
		switch (tx.kind) {
			case 'TransferObjects':
				// Ensure that we are only transferring results of a claim
				if (!tx.objects.every((o) => o.kind === 'Result' || o.kind === 'NestedResult')) {
					return false;
				}
				transfers++;
				break;
			case 'MoveCall':
				const [packageId, module, fn] = tx.target.split('::');

				if (packageId !== options.packageId) {
					return false;
				}

				if (module !== 'zk_bag') {
					return false;
				}

				if (fn !== 'init_claim' && fn !== 'reclaim' && fn !== 'claim' && fn !== 'finalize') {
					return false;
				}
				break;
			default:
				return false;
		}
	}

	return transfers === 1;
}
