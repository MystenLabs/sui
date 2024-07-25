// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ObjectOwner } from '@mysten/sui/client';

import type { Rpc_Object_FieldsFragment } from '../generated/queries.js';

export function mapGraphQLOwnerToRpcOwner(
	owner: Rpc_Object_FieldsFragment['owner'],
): ObjectOwner | null {
	switch (owner?.__typename) {
		case 'AddressOwner':
			return owner.owner?.asObject
				? {
						ObjectOwner: owner.owner?.asObject.address!,
					}
				: {
						AddressOwner: owner.owner?.asAddress?.address!,
					};
		case 'Parent':
			return {
				ObjectOwner: owner.parent?.address,
			};
		case 'Shared': {
			return {
				Shared: {
					initial_shared_version: String(owner.initialSharedVersion),
				},
			};
		}
		case 'Immutable':
			return 'Immutable';
	}

	return null;
}
