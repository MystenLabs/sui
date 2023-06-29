// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Info16, CheckStroke16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';

import LoadingIndicator from '_components/loading/LoadingIndicator';

import type { ReactNode } from 'react';

const alertStyles = cva(
	'rounded-2xl text-pBodySmall font-medium flex flex-row flex-nowrap justify-start items-center py-2 px-2.5 gap-2',
	{
		variants: {
			mode: {
				warning: 'border-solid border bg-warning-light border-warning-dark/20 text-warning-dark',
				success: 'border-solid border bg-success-light border-success-dark/20 text-success-dark',
				loading: 'bg-steel text-white border-warning-dark/20',
				issue: 'border-solid border bg-issue-light border-issue-dark/20 text-issue-dark',
			},
			noBorder: {
				true: '!border-transparent',
			},
			rounded: {
				lg: 'rounded-lg',
				xl: 'rounded-xl',
				'2xl': 'rounded-2xl',
			},
		},
		defaultVariants: {
			mode: 'issue',
			rounded: '2xl',
		},
	},
);

export interface AlertProps extends VariantProps<typeof alertStyles> {
	children: ReactNode;
}

const modeToIcon = {
	warning: <Info16 className="h-3.5 w-3.5" />,
	issue: <Info16 className="h-3.5 w-3.5" />,
	success: <CheckStroke16 className="h-3 w-3" />,
	loading: <LoadingIndicator color="inherit" />,
};

export default function Alert({ children, noBorder, rounded, mode = 'issue' }: AlertProps) {
	return (
		<div className={alertStyles({ noBorder, rounded, mode })}>
			{(mode && modeToIcon[mode]) || null}
			<div className="break-words flex-1 text-left">{children}</div>
		</div>
	);
}
