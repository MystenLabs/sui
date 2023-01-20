// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Check12 as CheckIcon } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { Banner, type BannerProps } from '../Banner';

export default {
    component: Banner,
    args: { onDismiss: undefined },
} as Meta;

export const Positive: StoryObj<BannerProps> = {
    args: {
        variant: 'positive',
        children: 'Positive',
    },
};

export const Warning: StoryObj<BannerProps> = {
    args: {
        variant: 'warning',
        children: 'Warning',
    },
};

export const Error: StoryObj<BannerProps> = {
    args: {
        variant: 'error',
        children: 'Error',
    },
};

export const Message: StoryObj<BannerProps> = {
    args: {
        variant: 'message',
        children: 'Message',
    },
};

export const LongMessage: StoryObj<BannerProps> = {
    args: {
        children: 'This is a very long message. '.repeat(20),
    },
};

export const LongMessageDismissible: StoryObj<BannerProps> = {
    args: {
        children: 'This is a very long message. '.repeat(20),
        onDismiss: () => null,
    },
};

export const CenteredFullWidth: StoryObj<BannerProps> = {
    args: {
        fullWidth: true,
        align: 'center',
        children: 'Message',
    },
};

export const CustomIcon: StoryObj<BannerProps> = {
    args: {
        icon: <CheckIcon />,
        children: 'Message',
    },
};

export const NoIcon: StoryObj<BannerProps> = {
    args: {
        icon: null,
        variant: 'message',
        children: 'Message',
    },
};

export const Dismissible: StoryObj<BannerProps> = {
    args: {
        fullWidth: false,
        children: 'Message',
        onDismiss: () => null,
    },
};

export const DismissibleFullWidth: StoryObj<BannerProps> = {
    args: {
        fullWidth: true,
        children: 'Message',
        onDismiss: () => null,
    },
};

export const DismissibleCenteredFullWidth: StoryObj<BannerProps> = {
    args: {
        fullWidth: true,
        align: 'center',
        children: 'Message',
        onDismiss: () => null,
    },
};
