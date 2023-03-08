// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { DelegatedStake } from '@mysten/sui.js';

// Helper function to get the delegation by stakedSuiId
export const getDelegationDataByStakeId = (
    delegationsStake: DelegatedStake[],
    stakeSuiId: string
) => {
    let delegation = null;
    for (const { delegations } of delegationsStake) {
        delegation =
            delegations.find(({ stakedSuiId }) => stakedSuiId === stakeSuiId) ||
            null;
        if (delegation) return delegation;
    }

    return delegation;
};
