// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { DateFilter, useDateFilterState, type DateFilterProps } from '../DateFilter';

export default {
	component: DateFilter,
} as Meta;

export const Default: StoryObj<DateFilterProps> = {
	render: (props) => {
		const [value, onChange] = useDateFilterState('D');
		return <DateFilter {...props} value={value} onChange={onChange} />;
	},
};

export const CustomOptions: StoryObj<DateFilterProps> = {
	...Default,
	args: { options: ['D', 'ALL'] },
};
