// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifie

import { type DelegatedStake } from '@mysten/sui.js';

// Get Stake SUI by stakeSuiId
export const getStakeSuiBySuiId = (
    allDelegation: DelegatedStake[],
    stakeSuiId?: string | null
) => {
    return (
        allDelegation.reduce((acc, curr) => {
            const total = BigInt(
                curr.delegations.find(
                    ({ stakedSuiId }) => stakedSuiId === stakeSuiId
                )?.principal || 0
            );
            return total + acc;
        }, 0n) || 0n
    );
};
