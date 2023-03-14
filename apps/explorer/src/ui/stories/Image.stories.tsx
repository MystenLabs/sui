// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Image, type ImageProps } from '../Image';

import type { Meta, StoryObj } from '@storybook/react';

export default {
    component: Image,
} as Meta;

export const Default: StoryObj<ImageProps> = {
    args: {
        src: 'https://images.unsplash.com/photo-1588466585717-f8041aec7875?ixlib=rb-4.0.3&ixid=MnwxMjA3fDB8MHxwaG90by1wYWdlfHx8fGVufDB8fHx8&auto=format&fit=crop&w=1374&q=80',
        alt: 'Goat',
    },
};

export const Sized: StoryObj<ImageProps> = {
    render: () => (
        <div className="flex space-x-2">
            <Image
                size="sm"
                rounded="md"
                src="https://images.unsplash.com/photo-1588466585717-f8041aec7875?auto=format&fit=crop&w=1374&q=80"
            />
            <Image
                size="md"
                rounded="md"
                src="https://images.unsplash.com/photo-1588466585717-f8041aec7875?auto=format&fit=crop&w=1374&q=80"
            />
            <Image
                size="lg"
                rounded="lg"
                src="https://images.unsplash.com/photo-1588466585717-f8041aec7875?auto=format&fit=crop&w=1374&q=80"
            />
        </div>
    ),
};

export const FallbackImage: StoryObj<ImageProps> = {
    args: {
        size: 'lg',
        rounded: 'lg',
        src: '',
    },
};

export const Blurred: StoryObj<ImageProps> = {
    args: {
        blur: true,
        src: 'https://images.unsplash.com/photo-1439130490301-25e322d88054?auto=format&fit=crop&w=1632&q=80',
        size: 'lg',
        rounded: 'lg',
    },
};
