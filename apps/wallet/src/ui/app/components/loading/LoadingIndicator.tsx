// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Spinner16 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';

const styles = cva('', {
	variants: {
		color: {
			inherit: 'text-inherit',
			sui: 'text-sui',
		},
	},
});

export type LoadingIndicatorProps = VariantProps<typeof styles>;

const LoadingIndicator = ({ color = 'sui' }: LoadingIndicatorProps) => {
	return <Spinner16 className={styles({ className: 'animate-spin', color })} />;
};

export default LoadingIndicator;
