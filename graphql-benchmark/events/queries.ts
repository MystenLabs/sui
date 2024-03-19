// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';

export const EventsQuery = graphql(`
	query queryEvents(
		$filter: EventFilter!
		$before: String
		$after: String
		$first: Int
		$last: Int
		$showSendingModule: Boolean = false
		$showContents: Boolean = false
	) {
		events(filter: $filter, first: $first, after: $after, last: $last, before: $before) {
			pageInfo {
				hasNextPage
				hasPreviousPage
				endCursor
				startCursor
			}
			nodes {
				sendingModule @include(if: $showSendingModule) {
					package {
						address
					}
					name
				}
				sender {
					address
				}
				type {
					repr
				}
				json @include(if: $showContents)
			}
		}
	}
`);

export const queries = { queryEvents: EventsQuery };
