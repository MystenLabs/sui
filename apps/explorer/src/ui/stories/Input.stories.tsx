// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Input, type InputProps } from '../Input';

import type { Meta, StoryObj } from '@storybook/react';

export default {
    component: Input,
} as Meta;

export const InputDefault: StoryObj<InputProps> = {
    render: (props) => <Input value="Test value" {...props} />,
};

export const InputPlaceholder: StoryObj<InputProps> = {
    ...InputDefault,
    args: {
        value: undefined,
        placeholder: 'Test placeholder',
    },
};

export const InputDisabled: StoryObj<InputProps> = {
    ...InputDefault,
    args: {
        disabled: true,
    },
};

export const InputWithLabel: StoryObj<InputProps> = {
    ...InputDefault,
    args: {
        label: 'Test Label',
    },
};
