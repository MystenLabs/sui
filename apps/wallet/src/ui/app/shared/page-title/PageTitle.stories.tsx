// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import PageTitle from './';

export default {
    component: PageTitle,
} as Meta<typeof PageTitle>;

export const Default: StoryObj<typeof PageTitle> = {
    args: {
        title: 'Title',
        stats: 'Stats',
        backLink: 'Back',
    },
};
