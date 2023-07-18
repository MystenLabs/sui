// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import * as Icons from '@mysten/icons';
import { type Meta } from '@storybook/react';

export default {
	title: '@mysten/icons',
} as Meta;

export const AllIcons = {
	render: () => (
		<div className="flex flex-col items-start gap-2">
			{Object.keys(Icons).map((key) => {
				const Icon = Icons[key as keyof typeof Icons];
				return (
					<div key={key} className="flex items-center gap-2">
						<Icon />
						<span>{key}</span>
					</div>
				);
			})}
		</div>
	),
};
