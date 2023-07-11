// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useRpcClient } from '../api/RpcClientContext';
import { type SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useGetCoinBalance(
	coinType: string,
	address?: SuiAddress | null,
	refetchInterval?: number,
	staleTime?: number,
) {
	const rpc = useRpcClient();

	return useQuery({
		queryKey: ['coin-balance', address, coinType],
		queryFn: () => rpc.getBalance({ owner: address!, coinType }),
		enabled: !!address && !!coinType,
		refetchInterval,
		staleTime,
	});
}
