// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export function useSystemState() {
    const rpc = useRpcClient();
    return useQuery(['system', 'state'], () => rpc.getLatestSuiSystemState());
}
