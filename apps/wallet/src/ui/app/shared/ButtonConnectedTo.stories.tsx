// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { StakeAdd16 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { ButtonConnectedTo } from './ButtonConnectedTo';

export default {
    component: ButtonConnectedTo,
} as Meta<typeof ButtonConnectedTo>;

export const Default: StoryObj<typeof ButtonConnectedTo> = {
    args: {
        text: 'Button',
    },
};

export const LightGrey: StoryObj<typeof ButtonConnectedTo> = {
    args: {
        text: 'Button',
        bgOnHover: 'grey',
    },
};

export const Disabled: StoryObj<typeof ButtonConnectedTo> = {
    args: {
        text: 'Button',
        bgOnHover: 'grey',
        disabled: true,
    },
};

export const LongText: StoryObj<typeof ButtonConnectedTo> = {
    render: (props) => {
        return (
            <div className="w-28">
                <ButtonConnectedTo {...props} />
            </div>
        );
    },
    args: {
        text: 'Button with very long text',
        bgOnHover: 'grey',
        iconBefore: <StakeAdd16 />,
        iconAfter: <StakeAdd16 />,
    },
};
