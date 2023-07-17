// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';

// Current API version is the same as the binary version
export function useGetBinaryVersion() {
	const rpc = useRpcClient();
	return useQuery({
		queryKey: ['binary-version'],
		queryFn: () => rpc.getRpcApiVersion(),
	});
}
