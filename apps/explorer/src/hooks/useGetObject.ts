// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

//TODO: (Jibz) - In another PR, remove this once merged into core
export function useGetSystemObject() {
    const rpc = useRpcClient();
    return useQuery(['system', 'state'], () => rpc.getLatestSuiSystemState());
}
