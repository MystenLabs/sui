// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { QREDO_CONNECTION_INFO_KEY_COMMON } from '../pages/qredo-connect/utils';
import { useBackgroundClient } from './useBackgroundClient';

export function useQredoInfo(qredoID?: string) {
    const backgroundClient = useBackgroundClient();
    return useQuery({
        queryKey: [...QREDO_CONNECTION_INFO_KEY_COMMON, qredoID],
        queryFn: async () => backgroundClient.getQredoConnectionInfo(qredoID!),
        enabled: !!qredoID,
        staleTime: Infinity,
        meta: { skipPersistedCache: true },
    });
}
