// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@mysten/sui.js/graphql/schemas/2024-01';

import { SuiGraphQLClient } from '@mysten/sui.js/graphql';
import { graphql, readFragment, ResultOf } from '@mysten/sui.js/graphql/schemas/2024-01';

const DYNAMIC_FIELD_TYPE =
	'0x0000000000000000000000000000000000000000000000000000000000000002::dynamic_field::Field<0x0000000000000000000000000000000000000000000000000000000000000002::dynamic_object_field::Wrapper<u16>,0x0000000000000000000000000000000000000000000000000000000000000002::object::ID>';

const CreatedLinkFragment = graphql(`
	fragment CreatedLinkFragment on TransactionBlock {
		digest
		effects {
			timestamp
			objectChanges {
				nodes {
					outputState {
						address
						version
						owner {
							__typename
							... on Parent {
								parent {
									address
									owner {
										... on Parent {
											parent {
												address
											}
										}
									}
								}
							}
							... on AddressOwner {
								owner {
									address
								}
							}
						}
						asMoveObject {
							contents {
								data
								type {
									repr
								}
							}
						}
					}
				}
			}
		}
	}
`);

const GetCreatedLinksQuery = graphql(
	`
		query getCreatedLinks($sender: String!, $cursor: String, $function: String!) {
			transactionBlocks(filter: { signAddress: $sender, function: $function }, last: 1) {
				nodes {
					...CreatedLinkFragment
				}
			}
		}
	`,
	[CreatedLinkFragment],
);

export async function getCreatedLinks(options: {
	network: 'mainnet' | 'testnet';
	sender: string;
	packageId: string;
	cursor?: string;
}) {
	const gqlClient = new SuiGraphQLClient({
		url:
			options.network === 'mainnet'
				? 'https://sui-mainnet.mystenlabs.com/graphql'
				: 'https://sui-testnet.mystenlabs.com/graphql',
	});

	const page = await gqlClient.query({
		query: GetCreatedLinksQuery,
		variables: {
			sender: options.sender,
			cursor: options.cursor,
			function: `${options.packageId}::zk_bag::new`,
		},
	});

	const transactionBlocks = page.data?.transactionBlocks;

	if (!transactionBlocks || page.errors?.length) {
		throw new Error('Failed to load created links');
	}

	// return {
	// 	cursor: transactionBlocks.pageInfo.startCursor,
	// 	hasNextPage: transactionBlocks.pageInfo.hasPreviousPage,
	// 	links: transactionBlocks.node.map((node: any) => node.id),
	// };
}

function listLinkAssets(txb: ResultOf<typeof CreatedLinkFragment>) {
	const nfts = [];
	const balances = new Map<string, bigint>();

	const dynamicField = txb.effects?.objectChanges.nodes.find((node) => {
		node.outputState?.asMoveObject?.contents?.type.repr === DYNAMIC_FIELD_TYPE;
	});

	if (!objectField) {
		throw new Error('Failed to find dynamic field');
	}

	const dynamicFieldId = objectField.
}
