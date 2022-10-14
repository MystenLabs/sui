// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { Placeholder } from '../Placeholder';

export default {
    title: 'UI/Placeholder',
    component: Placeholder,
} as ComponentMeta<typeof Placeholder>;

export const VaryingWidthAndHeight: ComponentStory<typeof Placeholder> = (
    args
) => (
    <div>
        <Placeholder width="120px" height="12px" />
        <br />
        <Placeholder width="90px" height="16px" />
        <br />
        <Placeholder width="59px" height="32px" />
    </div>
);
