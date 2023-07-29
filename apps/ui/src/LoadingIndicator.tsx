// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Spinner16, Spinner24 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';

const loadingIndicatorStyles = cva('animate-spin text-steel', {
	variants: {
		variant: {
			md: 'h-4 w-4',
			lg: 'h-6 w-6',
		},
	},
	defaultVariants: {
		variant: 'md',
	},
});

type LoadingIndicatorStylesProps = VariantProps<typeof loadingIndicatorStyles>;

export interface LoadingIndicatorProps extends LoadingIndicatorStylesProps {
	text?: string;
}

export function LoadingIndicator({ text, variant }: LoadingIndicatorProps) {
	const SpinnerIcon = variant === 'md' ? Spinner16 : Spinner24;

	return (
		<div className="inline-flex flex-row flex-nowrap items-center gap-3">
			<SpinnerIcon className={loadingIndicatorStyles({ variant })} />
			{text ? <div className="text-body font-medium text-steel-dark">{text}</div> : null}
		</div>
	);
}
