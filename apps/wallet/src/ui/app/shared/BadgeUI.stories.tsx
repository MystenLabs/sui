// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { BadgeUI } from './BadgeUI';

export default {
    component: BadgeUI,
} as Meta<typeof BadgeUI>;

export const Success: StoryObj<typeof BadgeUI> = {
    args: {
        label: 'New',
        variant: 'success',
    },
};

export const Warning: StoryObj<typeof BadgeUI> = {
    args: {
        label: 'At Risk',
        variant: 'warning',
    },
};
