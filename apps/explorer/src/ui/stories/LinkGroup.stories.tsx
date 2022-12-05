// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { LinkGroup, type LinkGroupProps } from '../LinkGroup';

export default {
    component: LinkGroup,
} as Meta;

export const Links: StoryObj<LinkGroupProps> = {
    args: {
        title: 'Link group with links',
        links: [
            { text: 'Link 1', to: '' },
            { text: 'Link 2', to: '' },
            { text: 'Link 3', to: '' },
        ],
    },
};

export const Text: StoryObj<LinkGroupProps> = {
    args: {
        title: 'Link group with text',
        text: 'Test text',
    },
};

export const EmptyLinks: StoryObj<LinkGroupProps> = {
    args: {
        title: 'Link group with empty links',
        links: [],
    },
};

export const EmptyText: StoryObj<LinkGroupProps> = {
    args: {
        title: 'Link group with empty text',
        text: '',
    },
};

export const NullText: StoryObj<LinkGroupProps> = {
    args: {
        title: 'Link group with null text',
        text: null,
    },
};
