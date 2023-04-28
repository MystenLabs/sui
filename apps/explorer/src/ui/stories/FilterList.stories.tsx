// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { FilterList, type FilterListProps } from '../FilterList';

export default {
    component: FilterList,
} as Meta;

export const Default: StoryObj<FilterListProps> = {
    render: (props) => {
        const [value, onChange] = useState('');
        return <FilterList {...props} value={value} onChange={onChange} />;
    },
    args: {
        options: ['MINT', 'SUI'],
        disabled: false,
        size: 'sm',
        lessSpacing: true,
    },
};
