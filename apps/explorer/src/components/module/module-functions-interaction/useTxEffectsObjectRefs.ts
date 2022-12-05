// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionEffects } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { SuiTransactionResponse } from '@mysten/sui.js';

export function useTxEffectsObjectRefs(
    tx: SuiTransactionResponse | null,
    objectType: 'created' | 'mutated' = 'created'
) {
    const txEffects = tx && getTransactionEffects(tx);
    return useMemo(
        () => txEffects?.[objectType]?.map((anObj) => anObj.reference) || [],
        [txEffects, objectType]
    );
}
