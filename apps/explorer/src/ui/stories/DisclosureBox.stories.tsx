// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { DisclosureBox, type DisclosureBoxProps } from '../DisclosureBox';

export default {
    component: DisclosureBox,
} as Meta;

export const DisclosureBoxDefault: StoryObj<DisclosureBoxProps> = {
    render: (props) => <DisclosureBox {...props}>Test content</DisclosureBox>,
    args: { title: 'Closed by default' },
};

export const DisclosureBoxClosed: StoryObj<DisclosureBoxProps> = {
    ...DisclosureBoxDefault,
    args: {
        title: 'Expanded disclosure box',
        defaultOpen: true,
        preview: 'Preview content',
    },
};
