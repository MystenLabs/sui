// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useContext, useMemo } from 'react';

import { NetworkContext } from '~/context';
import { DefaultRpcClient as rpc } from '~/utils/api/DefaultRpcClient';

// NOTE: This is intended to create a standard RPC interface to insulate consumers
// from internal implementation changes. We will refactor the RPC client to use a different
// approach at some point in the future, but this API should be constant through that.
export function useRpc() {
    const [network] = useContext(NetworkContext);
    return useMemo(() => rpc(network), [network]);
}
