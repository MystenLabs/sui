// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from '_hooks';

export function useGetObjectBatch(objectIds: string[]) {
    const rpc = useRpc();
    return useQuery(['get-object-batch', objectIds], () =>
        rpc.getObjectBatch(objectIds)
    );
}
