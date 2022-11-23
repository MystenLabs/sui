// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { DisplayBox, type DisplayBoxProps } from '../DisplayBox';

export default {
    component: DisplayBox,
} as Meta;

export const Default: StoryObj<DisplayBoxProps> = {
    args: {
        display:
            'https://sui-explorer-test-image.s3.amazonaws.com/testImage.png',
        caption: 'Example Image',
        fileInfo: 'PNG',
    },
};
