// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cva, type VariantProps } from 'class-variance-authority';

const appEmptyStyle = cva(['flex gap-3 p-3.75 h-28'], {
	variants: {
		displayType: {
			full: 'w-full',
			card: 'bg-white flex flex-col p-3.75 box-border w-full rounded-2xl h-32 box-border w-full rounded-2xl border border-solid border-gray-40',
		},
	},
	defaultVariants: {
		displayType: 'full',
	},
});

export interface SuiAppEmptyProps extends VariantProps<typeof appEmptyStyle> {}

export function SuiAppEmpty({ ...styleProps }: SuiAppEmptyProps) {
	return (
		<div className={appEmptyStyle(styleProps)}>
			<div className="bg-gray-40 w-10 h-10 rounded-full"></div>
			<div className="flex flex-col gap-2.5 flex-1">
				{styleProps.displayType === 'full' ? (
					<>
						<div className="bg-gray-40 rounded h-3.5 w-2/5"></div>
						<div className="bg-gray-40 rounded h-3.5 w-full"></div>
						<div className="bg-gray-40 rounded h-3.5 w-1/4"></div>
					</>
				) : (
					<div className="flex gap-2">
						<div className="bg-gray-40 rounded h-3.5 w-1/4"></div>
						<div className="bg-gray-40 rounded h-3.5 w-3/5"></div>
					</div>
				)}
			</div>
		</div>
	);
}
