// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type JsonRpcProvider } from '@mysten/sui.js';
import { createContext, useContext } from 'react';

export const RpcClientContext = createContext<JsonRpcProvider | undefined>(undefined);

export function useRpcClient() {
	const rpcClient = useContext(RpcClientContext);
	if (!rpcClient) {
		throw new Error('useRpcClient must be within RpcClientContext');
	}
	return rpcClient;
}
