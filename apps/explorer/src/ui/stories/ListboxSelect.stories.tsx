// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { ListboxSelect, type ListboxSelectPros } from '../ListboxSelect';

export default {
    component: ListboxSelect,
} as Meta;

export const Default: StoryObj<ListboxSelectPros> = {
    render: (props) => {
        const [value, onChange] = useState('Option 1');
        return (
            <div className="flex justify-center">
                <ListboxSelect {...props} value={value} onSelect={onChange} />
            </div>
        );
    },
    args: {
        options: ['Option 1', 'Option 2', 'Option 3', 'Long option'],
    },
};
