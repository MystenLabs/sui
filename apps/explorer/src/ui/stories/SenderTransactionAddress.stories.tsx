// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { type ComponentProps } from 'react';
import { MemoryRouter } from 'react-router-dom';

import { SenderTransactionAddress } from '../TransactionAddressSection';

export default {
    component: SenderTransactionAddress,
    decorators: [
        (Story) => (
            <QueryClientProvider client={new QueryClient()}>
                <MemoryRouter>
                    <Story />
                </MemoryRouter>
            </QueryClientProvider>
        ),
    ],
} as Meta;

export const Default: StoryObj<
    ComponentProps<typeof SenderTransactionAddress>
> = {
    args: {
        sender: '0x813f1adee5abb1e00dfa653bb827856106e56764',
    },
};
