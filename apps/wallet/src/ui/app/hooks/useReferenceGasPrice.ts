// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from './useRpc';

export function useReferenceGasPrice() {
    const rpc = useRpc();
    // TODO: when epoch changes we should clear cache for this
    return useQuery(['getReferenceGasPrice'], () => rpc.getReferenceGasPrice());
}
