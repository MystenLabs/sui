// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';

import { Placeholder, type PlaceholderProps } from '../Placeholder';

export default {
	component: Placeholder,
} as Meta;

export const VaryingWidthAndHeight: StoryObj<PlaceholderProps> = {
	render: () => (
		<div>
			<Placeholder width="120px" height="12px" />
			<br />
			<Placeholder width="90px" height="16px" />
			<br />
			<Placeholder width="59px" height="32px" />
		</div>
	),
};
