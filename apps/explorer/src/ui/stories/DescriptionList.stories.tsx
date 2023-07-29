// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';
import { type Meta, type StoryObj } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { DescriptionList, DescriptionItem, type DescriptionListProps } from '../DescriptionList';
import { Link } from '~/ui/Link';

export default {
	component: DescriptionList,
	decorators: [
		(Story) => (
			<MemoryRouter>
				<Story />
			</MemoryRouter>
		),
	],
} as Meta;

export const Default: StoryObj<DescriptionListProps> = {
	render: () => (
		<DescriptionList>
			<DescriptionItem title="Object ID">
				<Link variant="mono" to="/">
					0xb758af2061e7c0e55df23de52c51968f6efbc959
				</Link>
			</DescriptionItem>
			<DescriptionItem title={<Text variant="bodySmall/medium">Owner</Text>}>
				Value 1
			</DescriptionItem>
		</DescriptionList>
	),
};
