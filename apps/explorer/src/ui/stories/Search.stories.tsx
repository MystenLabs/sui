// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Search, type SearchProps } from '../Search';

export default {
    component: Search,
} as Meta;

export const Default: StoryObj<SearchProps> = {
    args: {},
    render: () => (
        <div className="flex h-screen w-screen bg-headerNav p-10">
            <Search />
        </div>
    ),
};
