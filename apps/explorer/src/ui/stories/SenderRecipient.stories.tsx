// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
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
                symbol: 'SUI',
            },
        },
        {
            address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',
            coin: {
                amount: '1.345M',
                symbol: 'COIN',
            },
        },
        {
            address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',
            coin: {
                amount: 100000230404050504,
                symbol: 'MIST',
            },
        },
        {
            address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',
            coin: {
                amount: 1000002,
            },
        },
        {
            address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
        },
    ],
};

export default {
    component: SenderRecipient,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
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
