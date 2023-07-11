// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type DelegatedStake } from '@mysten/sui.js';

// Get staked Sui
export const getAllStakeSui = (allDelegation: DelegatedStake[]) => {
	return (
		allDelegation.reduce(
			(acc, curr) => curr.stakes.reduce((total, { principal }) => total + BigInt(principal), acc),
			0n,
		) || 0n
	);
};
