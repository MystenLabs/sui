// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';

import { PageHeader, type PageHeaderProps } from '../PageHeader';

export default {
    component: PageHeader,
} as Meta;

const title = '0x76763c665d5de1f59471e87af92767f3df376580';

export const Address: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Address',
    },
};

export const CallTransaction: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Call',
        status: 'success',
    },
};

export const CallTransactionFailure: StoryObj<PageHeaderProps> = {
    args: {
        ...CallTransaction.args,
        status: 'failure',
    },
};

export const ChangeEpoch: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'ChangeEpoch',
    },
};

export const Checkpoint: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Checkpoint',
    },
};

export const Object: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Object',
    },
};

export const Package: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Package',
        subtitle: 'Package Name',
    },
};

export const PackageLongSubtitle: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Package',
        subtitle: title,
    },
};

export const Pay: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'Pay',
    },
};

export const TransferObject: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'TransferObject',
        status: 'success',
    },
};

export const TransferSui: StoryObj<PageHeaderProps> = {
    args: {
        title,
        type: 'TransferSui',
        status: 'success',
    },
};
