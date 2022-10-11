// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import useAppSelector from './useAppSelector';

export function useObjectsState() {
    const objectsLoading = useAppSelector(
        ({ suiObjects }) => suiObjects.loading
    );
    const lastSync = useAppSelector(({ suiObjects }) => suiObjects.lastSync);
    const error = useAppSelector(({ suiObjects }) => suiObjects.error);
    const showError =
        !!error && (!lastSync || Date.now() - lastSync > 30 * 1000);
    const syncedOnce = !!lastSync;
    const loading = objectsLoading && !syncedOnce && !error;
    return useMemo(
        () => ({
            loading,
            syncedOnce,
            error,
            showError,
        }),
        [loading, syncedOnce, error, showError]
    );
}
