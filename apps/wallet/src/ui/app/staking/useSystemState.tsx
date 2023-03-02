// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from '../hooks';

export function useSystemState() {
    const rpc = useRpc();
    return useQuery(['system', 'state'], () => rpc.getSuiSystemState());
}
