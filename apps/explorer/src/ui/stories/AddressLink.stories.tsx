// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { AddressLink, type AddressLinkProps } from '../AddressLink';

export default {
    component: AddressLink,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const shortenLink: StoryObj<AddressLinkProps> = {
    args: {
        text: '0x76763c665d5...2767f3df376580',
        link: '0x76763c665d5de1f59471e87af92767f3df376580',
    },
};

export const fullLink: StoryObj<AddressLinkProps> = {
    args: {
        link: '0x76763c665d5de1f59471e87af92767f3df376580',
    },
};
