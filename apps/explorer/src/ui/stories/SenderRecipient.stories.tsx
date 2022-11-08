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
            amount: {
                value: 1000,
                unit: 'SUI',
            },
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
        transferCoin: true,
        sender: '0x813f1adee5abb1e00dfa653bb827856106e56764',
        recipients: [
            {
                address: '0x955d8ddc4a17670bda6b949cbdbc8f5aac820cc7',
                amount: {
                    value: 1000,
                    unit: 'SUI',
                },
            },
        ],
    },
};

export const noRecipient: StoryObj<SenderRecipientProps> = {
    args: {
        sender: '0x813f1adee5abb1e00dfa653bb827856106e56764',
        transferCoin: false,
        recipients: [],
    },
};

export const multipleRecipients: StoryObj<SenderRecipientProps> = {
    args: {
        ...data,
        transferCoin: false,
        recipients: [
            {
                address: '0x955d8ddc4a17670bda6b949cbdbc8f5aac820cc7',
                amount: {
                    value: 400,
                    unit: 'SUI',
                },
            },
            {
                address: '0x9798852b55fcbf352052c9414920ebf7811ce05e',
                amount: {
                    value: 1850,
                    unit: 'COIN',
                },
            },
            {
                address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
            },
        ],
    },
};
