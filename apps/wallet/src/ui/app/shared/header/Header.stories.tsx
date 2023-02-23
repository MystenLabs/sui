// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Header } from './Header';

export default {
    component: Header,
} as Meta<typeof Header>;

export const Default: StoryObj<typeof Header> = {};

export const WithMiddleContent: StoryObj<typeof Header> = {
    args: {
        middleContent: (
            <div className="text-ellipsis whitespace-nowrap overflow-hidden">
                Connected to some dapp
            </div>
        ),
    },
};

export const WithRightContent: StoryObj<typeof Header> = {
    args: {
        rightContent: <div>Menu</div>,
    },
};

export const WithAll: StoryObj<typeof Header> = {
    args: {
        middleContent: (
            <div className="text-ellipsis whitespace-nowrap overflow-hidden">
                Connected to some dapp
            </div>
        ),
        rightContent: <div>Menu</div>,
    },
};
