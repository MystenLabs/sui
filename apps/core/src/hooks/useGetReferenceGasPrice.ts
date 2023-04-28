// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

export function useGetReferenceGasPrice() {
    const rpc = useRpcClient();
    return useQuery(['current-epoch', 'gas-price'], () =>
        rpc.getReferenceGasPrice()
    );
}
