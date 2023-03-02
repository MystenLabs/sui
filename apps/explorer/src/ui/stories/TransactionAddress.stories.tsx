// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import {
    TransactionAddress,
    type TransactionAddressProps,
} from '../TransactionAddress';

export default {
    component: TransactionAddress,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Default: StoryObj<TransactionAddressProps> = {
    args: {
        address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
    },
};

export const WithIcon: StoryObj<TransactionAddressProps> = {
    args: {
        address: '0x813f1adee5abb1e00dfa653bb827856106e56764',
        icon: <CheckFill16 />,
    },
};
