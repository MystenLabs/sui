// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Toaster } from 'react-hot-toast';

import type { Meta, StoryObj } from '@storybook/react';

import {
    CopyToClipboard,
    type CopyToClipboardProps,
} from '~/ui/CopyToClipboard';

export default {
    component: CopyToClipboard,
} as Meta;

export const Default: StoryObj<CopyToClipboardProps> = {
    render: () => (
        <div className="flex gap-2">
            <Toaster />
            <CopyToClipboard size="sm" copyText="Copy me!" />
            <CopyToClipboard size="md" copyText="Copy me!" />
            <CopyToClipboard size="lg" copyText="Copy me!" />
        </div>
    ),
};
