// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Toaster } from 'react-hot-toast';

import type { Meta, StoryObj } from '@storybook/react';

import { CopyClipboard, type CopyToClipboardProps } from '~/ui/CopyToClipboard';

export default {
    component: CopyClipboard,
} as Meta;

export const Default: StoryObj<CopyToClipboardProps> = {
    render: () => (
        <>
            <Toaster />
            <CopyClipboard copyText="Copy me!" />
        </>
    ),
};
