// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';

import { PlaceholderTable, type PlaceholderTableProps } from '../PlaceholderTable';

export default {
	component: PlaceholderTable,
} as Meta;

export const VaryingWidth: StoryObj<PlaceholderTableProps> = {
	render: () => (
		<PlaceholderTable
			rowCount={5}
			rowHeight="16px"
			colHeadings={['Sardine', 'Herring', 'Salmon', 'Barracuda']}
			colWidths={['38px', '90px', '120px', '204px']}
		/>
	),
};
