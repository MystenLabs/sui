// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

export function useGetEpochs(epoch: number, descendingOrder = false) {
    const rpc = useRpcClient();
    return useQuery(
        ['epochs', epoch],
        () =>
            rpc.getEpochs({
                cursor: epoch.toString(),
                limit: 1,
                descendingOrder,
            }),
        {
            enabled: !!epoch,
        }
    );
}
