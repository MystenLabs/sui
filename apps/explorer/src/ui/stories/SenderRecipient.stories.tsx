// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';

import { SenderRecipient, type SenderRecipientProps } from '../SenderRecipient';

const data = {
    transferCoin: false,
    sender: '0x813f1adee5abb1e00dfa653bb827856106e56764',
    recipients: [
        {
            address: '0x955d8ddc4a17670bda6b949cbdbc8f5aac820cc7',
            coin: {
                amount: 1000,
                symbol: '0x2::sui::SUI',
            },
        },
        {
            address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',
            coin: {
                amount: 120_030,
                symbol: 'COIN',
            },
        },
        {
            address: '0xc4173a804406a365e69dfb297d4eaaf002546ebd',
            coin: {
                amount: 10_050_504,
                symbol: 'MIST',
            },
        },
        {
            address: '0xca1e11744de126dd1b116c6a16df4715caea56a3',
            coin: {
                amount: '1000002',
            },
        },
        {
            address: '0x49e095bc33fda565c07937478f201f4344941f03',
        },
    ],
};

export default {
    component: SenderRecipient,
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

export const singleTransfer: StoryObj<SenderRecipientProps> = {
    args: {
        ...data,
        transferCoin: true,
        recipients: data.recipients.slice(0, 1),
    },
};

export const noRecipient: StoryObj<SenderRecipientProps> = {
    args: {
        ...data,
        transferCoin: false,
        recipients: [],
    },
};

export const multipleRecipients: StoryObj<SenderRecipientProps> = {
    args: {
        ...data,
    },
};
