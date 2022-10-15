// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';

import { PageHeader, type PageHeaderProps } from '../PageHeader';

export default {
    component: PageHeader,
} as Meta;

export const Default: StoryObj<PageHeaderProps> = {
    args: {
        title: '0x76763c665d5de1f59471e87af92767f3df376580',
        status: 'success',
    },
};
