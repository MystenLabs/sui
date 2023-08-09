// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useRpcClient } from '@mysten/core';
import { SuiClient } from '@mysten/sui.js/client';
import { useMemo } from 'react';

import { useNetwork } from '~/context';
import { Network } from '~/utils/api/DefaultRpcClient';

// TODO: Use enhanced RPC locally by default
export function useEnhancedRpcClient() {
	const [network] = useNetwork();
	const rpc = useRpcClient();
	const enhancedRpc = useMemo(() => {
		if (network === Network.LOCAL) {
			return new SuiClient({ url: 'http://localhost:9124' });
		}

		return rpc;
	}, [network, rpc]);

	return enhancedRpc;
}
