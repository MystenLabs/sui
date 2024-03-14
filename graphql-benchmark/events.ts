// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';

// with cursor
// by sender
// by transaction digest
// by emitting module
// by event type

/*
Combinations
Choosing 1 Item: 4 combinations

sender
tx_digest
emittingModule
eventType
Choosing 2 Items: 6 combinations

sender, tx_digest
sender, emittingModule
sender, eventType
tx_digest, emittingModule
tx_digest, eventType
emittingModule, eventType
Choosing 3 Items: 4 combinations

sender, tx_digest, emittingModule
sender, tx_digest, eventType
sender, emittingModule, eventType
tx_digest, emittingModule, eventType
Choosing All 4 Items: 1 combination

sender, tx_digest, emittingModule, eventType
Choosing None: 1 combination (though not a filtering scenario, it represents querying without any specific filter).


And note that these will always be bounded by a checkpoint_sequence_number, and potentially tx_sequence_number + event_sequence_number
*/

export const EventsQuery = graphql(`
query queryEvents(
	$filter: EventFilter!
	# filter missing:
	# - MoveEventField
	# - TimeRange
	# - All, Any, And, Or
	# missing order
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
            sendingModule @include(if: $showSendingModule){
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
}`);
