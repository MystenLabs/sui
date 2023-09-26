// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Add16, StakeAdd16 } from '@mysten/icons';
import { type Meta, type StoryObj } from '@storybook/react';

import { Button } from './ButtonUI';

export default {
	component: Button,
} as Meta<typeof Button>;

export const Default: StoryObj<typeof Button> = {
	args: {
		text: 'Default Button',
	},
};

export const AllButton: StoryObj<typeof Button> = {
	render: (props) => {
		const variants = [
			'primary',
			'secondary',
			'outline',
			'outlineWarning',
			'warning',
			'plain',
		] as const;
		const sizes = ['tall', 'narrow', 'xs'] as const;
		return (
			<div className="grid gap-4 grid-cols-2">
				{sizes.map((size) =>
					variants.map((variant) => (
						<div className="flex flex-col gap-2" key={variant + size}>
							<div className="text-bodySmall">{`${variant}-${size}`}</div>
							<Button
								{...{ variant, size, text: variant }}
								{...props}
								before={<StakeAdd16 />}
								after={<Add16 />}
							/>
							<Button
								{...{ variant, size, text: variant }}
								{...props}
								disabled
								text={`${props.text || variant} disabled`}
								before={<StakeAdd16 />}
								after={<Add16 />}
							/>
							<Button
								{...{ variant, size, text: variant }}
								{...props}
								loading
								text={`${props.text || variant} loading`}
								before={<StakeAdd16 />}
								after={<Add16 />}
							/>
						</div>
					)),
				)}
			</div>
		);
	},
};

export const AllLink: StoryObj<typeof Button> = {
	...AllButton,
	args: { to: '.' },
};
export const AllAnchor: StoryObj<typeof Button> = {
	...AllButton,
	args: { href: 'https://example.com' },
};
