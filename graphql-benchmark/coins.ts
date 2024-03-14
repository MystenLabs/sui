// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';

export const PageInfoFragment = graphql(`
	fragment PageInfo on PageInfo {
		startCursor
		endCursor
		hasPreviousPage
		hasNextPage
	}
`);

const CoinFragment = graphql(`
	fragment CoinData on Coin {
		coinBalance
		address
		version
		digest
		contents @include(if: $showContent) {
			type {
				repr
			}
		}
		previousTransactionBlock @include(if: $showPreviousTransaction) {
			digest
		}
	}
`);

export const CoinConnectionData = graphql(
	`
		fragment CoinConnectionData on CoinConnection {
			pageInfo {
				...PageInfo
			}
			nodes {
				...CoinData
			}
		}
	`,
	[PageInfoFragment, CoinFragment],
);

export const CoinsQuery = graphql(
	`
		query Coins(
			$first: Int
			$after: String
			$last: Int
			$before: String
			$type: String = "0x2::sui::SUI"
			$showContent: Boolean = false
			$showPreviousTransaction: Boolean = false
		) {
			coins(first: $first, after: $after, last: $last, before: $before, type: $type) {
				...CoinConnectionData
			}
		}
	`,
	[CoinConnectionData],
);
