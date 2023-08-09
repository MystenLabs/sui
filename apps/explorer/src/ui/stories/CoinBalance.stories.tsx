// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RpcClientContext } from '@mysten/core';
import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { CoinBalance, type CoinBalanceProps } from '../CoinBalance';
import { DefaultRpcClient, Network } from '~/utils/api/DefaultRpcClient';

export default {
	component: CoinBalance,
	decorators: [
		(Story) => (
			<QueryClientProvider client={new QueryClient()}>
				<RpcClientContext.Provider value={DefaultRpcClient(Network.LOCAL)}>
					<Story />
				</RpcClientContext.Provider>
			</QueryClientProvider>
		),
	],
} as Meta;

export const Default: StoryObj<CoinBalanceProps> = {
	args: {
		amount: 1000,
		coinType: '0x2::sui::SUI',
	},
};

export const WithoutSymbol: StoryObj<CoinBalanceProps> = {
	args: {
		amount: 10000,
	},
};
