// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClientProvider } from '@mysten/dapp-kit';
import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { CoinBalance, type CoinBalanceProps } from '../CoinBalance';
import { Network, NetworkConfigs, createSuiClient } from '~/utils/api/DefaultRpcClient';

export default {
	component: CoinBalance,
	decorators: [
		(Story) => (
			<QueryClientProvider client={new QueryClient()}>
				<SuiClientProvider
					networks={NetworkConfigs}
					defaultNetwork={Network.LOCAL}
					createClient={createSuiClient}
				>
					<Story />
				</SuiClientProvider>
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
