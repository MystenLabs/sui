// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { DelegatedStake } from '@mysten/sui/client';

import type { Rpc_Stake_FieldsFragment } from '../generated/queries.js';

export function mapGraphQLStakeToRpcStake(stakes: Rpc_Stake_FieldsFragment[]): DelegatedStake[] {
	const delegatedStakes = new Map<string, DelegatedStake>();

	for (const stake of stakes) {
		const pool = stake.contents?.json.pool_id as string;
		if (!delegatedStakes.has(pool)) {
			delegatedStakes.set(pool, {
				validatorAddress: '', // TODO
				stakingPool: pool,
				stakes: [],
			});
		}

		const delegatedStake = delegatedStakes.get(pool)!;
		delegatedStake.stakes.push({
			stakedSuiId: stake.address,
			stakeRequestEpoch: stake.requestedEpoch?.epochId.toString()!,
			stakeActiveEpoch: stake.activatedEpoch?.epochId.toString()!,
			principal: stake.principal?.value,
			status: (stake.stakeStatus.slice(0, 1).toUpperCase() +
				stake.stakeStatus.slice(1).toLowerCase()) as 'Active',
			estimatedReward: stake.estimatedReward?.value,
		});
	}

	return [...delegatedStakes.values()];
}
