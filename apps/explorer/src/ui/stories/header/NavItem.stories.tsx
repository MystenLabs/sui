// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Check24 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { NavItem, type NavItemProps } from '../../header/NavItem';

export default {
    component: NavItem,
    decorators: [
        (Story) => (
            <div className="bg-headerNav p-6">
                <Story />
            </div>
        ),
    ],
} as Meta;

export const Default: StoryObj<NavItemProps> = {
    args: {
        children: 'Nav Item',
    },
};

export const BeforeIcon: StoryObj<NavItemProps> = {
    args: {
        beforeIcon: <Check24 />,
        children: 'Nav Item',
    },
};
