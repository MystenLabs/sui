// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RpcClientContext } from '@mysten/core';
import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type ComponentProps } from 'react';
import { MemoryRouter } from 'react-router-dom';

import { RecipientTransactionAddresses } from '../TransactionAddressSection';

import { DefaultRpcClient, Network } from '~/utils/api/DefaultRpcClient';

const recipientsData = [
    {
        address: '0x955d8ddc4a17670bda6b949cbdbc8f5aac820cc7',
        amount: 1000,
        coinType: '0x2::sui::SUI',
    },
    {
        address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',

        amount: 120_030,
        coinType: 'COIN',
    },
    {
        address: '0xc4173a804406a365e69dfb297d4eaaf002546ebd',
        amount: 10_050_504,
        coinType: 'MIST',
    },
    {
        address: '0xca1e11744de126dd1b116c6a16df4715caea56a3',
        amount: 1000002,
    },
    {
        address: '0x49e095bc33fda565c07937478f201f4344941f03',
    },
];

export default {
    component: RecipientTransactionAddresses,
    decorators: [
        (Story) => (
            <QueryClientProvider client={new QueryClient()}>
                <RpcClientContext.Provider
                    value={DefaultRpcClient(Network.LOCAL)}
                >
                    <MemoryRouter>
                        <Story />
                    </MemoryRouter>
                </RpcClientContext.Provider>
            </QueryClientProvider>
        ),
    ],
} as Meta;

export const Default: StoryObj<
    ComponentProps<typeof RecipientTransactionAddresses>
> = {
    args: {
        recipients: recipientsData,
    },
};

export const singleRecipient: StoryObj<
    ComponentProps<typeof RecipientTransactionAddresses>
> = {
    args: {
        recipients: recipientsData.slice(0, 1),
    },
};
