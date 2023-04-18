// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

export function useGetSystemState() {
    const rpc = useRpcClient();
    return useQuery(['system', 'state'], () => rpc.getLatestSuiSystemState(), {
        select: (data) => ({
            ...data,
            activeValidators: data.activeValidators.sort((a, b) =>
                Math.random() > 0.5 ? -1 : 1
            ),
        }),
    });
}
