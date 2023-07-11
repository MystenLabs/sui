// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';
import { bcs } from '../bcs';
import { TRANSFER_POLICY_CREATED_EVENT, TRANSFER_POLICY_TYPE, TransferPolicy } from '../types';

/**
 * Searches the `TransferPolicy`-s for the given type. The seach is performed via
 * the `TransferPolicyCreated` event. The policy can either be owned or shared,
 * and the caller needs to filter the results accordingly (ie single owner can not
 * be accessed by anyone but the owner).
 *
 * @param provider
 * @param type
 */
export async function queryTransferPolicy(
	provider: JsonRpcProvider,
	type: string,
): Promise<TransferPolicy[]> {
	// console.log('event type: %s', `${TRANSFER_POLICY_CREATED_EVENT}<${type}>`);
	const { data } = await provider.queryEvents({
		query: {
			MoveEventType: `${TRANSFER_POLICY_CREATED_EVENT}<${type}>`,
		},
	});

	const search = data.map((event) => event.parsedJson as { id: string });
	const policies = await provider.multiGetObjects({
		ids: search.map((policy) => policy.id),
		options: { showBcs: true, showOwner: true },
	});

	return policies
		.filter((policy) => !!policy && 'data' in policy)
		.map(({ data: policy }) => {
			// should never happen; policies are objects and fetched via an event.
			// policies are filtered for null and undefined above.
			if (!policy || !policy.bcs || !('bcsBytes' in policy.bcs)) {
				throw new Error(`Invalid policy: ${policy?.objectId}, expected object, got package`);
			}

			let parsed = bcs.de(TRANSFER_POLICY_TYPE, policy.bcs.bcsBytes, 'base64');

			return {
				id: policy?.objectId,
				type: `${TRANSFER_POLICY_TYPE}<${type}>`,
				owner: policy?.owner!,
				rules: parsed.rules,
				balance: parsed.balance,
			} as TransferPolicy;
		});
}
