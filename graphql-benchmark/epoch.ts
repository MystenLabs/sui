// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { graphql } from '@mysten/sui.js/graphql/schemas/2024-01';

export const SingleEpoch = graphql(`
	query AtEpoch($id: Int) {
		epoch(id: $id) {
			epochId
		}
	}
`);
