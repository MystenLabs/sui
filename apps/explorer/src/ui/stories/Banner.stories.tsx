// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Banner, type BannerProps } from '../Banner';
import { ReactComponent as CheckIcon } from '../icons/check_12x12.svg';

export default {
	component: Banner,
	args: { onDismiss: undefined },
} as Meta;

export const Positive: StoryObj<BannerProps> = {
	args: {
		variant: 'positive',
		children: 'Positive',
		border: true,
	},
};

export const Warning: StoryObj<BannerProps> = {
	args: {
		variant: 'warning',
		children: 'Warning',
	},
};

export const Error: StoryObj<BannerProps> = {
	args: {
		variant: 'error',
		children: 'Error',
	},
};

export const Message: StoryObj<BannerProps> = {
	args: {
		variant: 'message',
		children: 'Message',
	},
};

export const NeutralGrey: StoryObj<BannerProps> = {
	args: {
		variant: 'neutralGrey',
		children: 'Neutral Grey',
		border: false,
	},
};

export const NeutralWhite: StoryObj<BannerProps> = {
	args: {
		variant: 'neutralWhite',
		children: 'Neutral White',
	},
};

export const LongMessage: StoryObj<BannerProps> = {
	args: {
		children: 'This is a very long message. '.repeat(20),
	},
};

export const LongMessageDismissible: StoryObj<BannerProps> = {
	args: {
		children: 'This is a very long message. '.repeat(20),
		onDismiss: () => null,
	},
};

export const CenteredFullWidth: StoryObj<BannerProps> = {
	args: {
		fullWidth: true,
		align: 'center',
		children: 'Message',
	},
};

export const CustomIcon: StoryObj<BannerProps> = {
	args: {
		icon: <CheckIcon />,
		children: 'Message',
	},
};

export const NoIcon: StoryObj<BannerProps> = {
	args: {
		icon: null,
		variant: 'message',
		children: 'Message',
	},
};

export const Dismissible: StoryObj<BannerProps> = {
	args: {
		fullWidth: false,
		children: 'Message',
		onDismiss: () => null,
	},
};

export const DismissibleFullWidth: StoryObj<BannerProps> = {
	args: {
		fullWidth: true,
		children: 'Message',
		onDismiss: () => null,
	},
};

export const DismissibleCenteredFullWidth: StoryObj<BannerProps> = {
	args: {
		fullWidth: true,
		align: 'center',
		children: 'Message',
		onDismiss: () => null,
	},
};

const variants = ['positive', 'warning', 'error', 'message', 'neutralGrey', 'neutralWhite'];

export const BannersWithBorder = {
	render: () => (
		<div className="flex flex-col gap-2">
			{variants.map((variant) => (
				<Banner key={variant} border shadow variant={variant as any}>
					<div className="capitalize">{variant}</div>
				</Banner>
			))}
		</div>
	),
};
