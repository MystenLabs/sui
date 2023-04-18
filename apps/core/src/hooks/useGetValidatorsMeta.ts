// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

// TODO: This is a temporary solution to get validators
// Use getLatestSuiSystemState whenever epoch is not provided
// Use getEpochs when epoch is provided
export function useGetValidatorsMeta(epoch?: number) {
    const rpc = useRpcClient();
    return useQuery(['validators', epoch], async () => {
        if (epoch) {
            const res = await rpc.getEpochs({
                cursor: epoch.toString(),
                limit: 1,
            });
            return res.data?.[0].validators;
        }
        return (await rpc.getLatestSuiSystemState()).activeValidators;
    });
}
