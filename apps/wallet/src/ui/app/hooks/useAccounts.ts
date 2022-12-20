// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useSyncExternalStore } from 'react';

import { SyncedStore } from '../helpers/SyncedStore';

import type { AccountSerialized } from '_src/background/keyring/Account';

export const accountsStore = new SyncedStore<AccountSerialized[] | null>(null);

export function useAccounts(addressesFilters?: string[]) {
    const accounts = useSyncExternalStore(
        accountsStore.subscribe,
        accountsStore.getSnapshot
    );
    return useMemo(() => {
        if (!accounts) {
            return null;
        }
        if (!addressesFilters?.length) {
            return accounts;
        }
        return accounts.filter((anAccount) =>
            addressesFilters.includes(anAccount.address)
        );
    }, [accounts, addressesFilters]);
}
