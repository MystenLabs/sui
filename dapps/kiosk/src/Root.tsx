// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from 'react-router-dom';
import { Toaster } from 'react-hot-toast';
import { WalletKitProvider } from '@mysten/wallet-kit';
import { Header } from './components/Base/Header';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { RpcClientContext } from './context/RpcClientContext';
import { JsonRpcProvider, testnetConnection } from '@mysten/sui.js';

const queryClient = new QueryClient();
const rpcProvider = new JsonRpcProvider(testnetConnection);

export default function Root() {
	return (
		<WalletKitProvider>
			<QueryClientProvider client={queryClient}>
				<RpcClientContext.Provider value={rpcProvider}>
					<Header></Header>
					<div className="min-h-[80vh]">
						<Outlet />
					</div>
					<div className="mt-6 border-t border-primary text-center py-6">
						Copyright © Mysten Labs, Inc.
					</div>
					<Toaster position="bottom-center" />
				</RpcClientContext.Provider>
			</QueryClientProvider>
		</WalletKitProvider>
	);
}
