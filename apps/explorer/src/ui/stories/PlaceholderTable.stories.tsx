// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { PlaceholderTable } from '../PlaceholderTable';

export default {
    title: 'UI/PlaceholderTable',
    component: PlaceholderTable,
} as ComponentMeta<typeof PlaceholderTable>;

export const VaryingWidth: ComponentStory<typeof PlaceholderTable> = (args) => (
    <PlaceholderTable
        rowCount={5}
        rowHeight="16px"
        colHeadings={['Sardine', 'Herring', 'Salmon', 'Barracuda']}
        colWidths={['38px', '90px', '120px', '204px']}
    />
);
