// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';

import { SyncedStore } from '../helpers/SyncedStore';

import type { SuiAddress } from '@mysten/sui.js';

export const activeAddressStore = new SyncedStore<SuiAddress | null>(null);

export function useActiveAddress() {
    return useSyncExternalStore(
        activeAddressStore.subscribe,
        activeAddressStore.getSnapshot
    );
}
