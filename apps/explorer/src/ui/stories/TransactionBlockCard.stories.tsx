// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { CollapsibleCard, type CollapsibleCardProps } from '~/ui/collapsible/CollapsibleCard';
import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

export default {
	component: CollapsibleCard,
} as Meta;

export const Default: StoryObj<CollapsibleCardProps> = {
	render: (props) => {
		const sections = Array(5)
			.fill(true)
			.map((_, index) => <div key={index}>Section Item {index}</div>);

		return (
			<div className="h-[1000px]">
				<CollapsibleCard collapsible title="Card Title" {...props}>
					{sections.map((section, index) => (
						<CollapsibleSection key={index} title={`Section Title ${index}`}>
							{section}
						</CollapsibleSection>
					))}
				</CollapsibleCard>
			</div>
		);
	},
};

export const Small: StoryObj<CollapsibleCardProps> = {
	...Default,
	args: { size: 'sm' },
};
