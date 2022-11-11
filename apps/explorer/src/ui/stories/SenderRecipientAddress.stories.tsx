// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import {
    SenderRecipientAddress,
    type SenderRecipientAddressProps,
} from '../SenderRecipientAddress';

export default {
    component: SenderRecipientAddress,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Sender: StoryObj<SenderRecipientAddressProps> = {
    args: {
        address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
        isSender: true,
    },
};

export const Recipient: StoryObj<SenderRecipientAddressProps> = {
    args: {
        address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
    },
};
