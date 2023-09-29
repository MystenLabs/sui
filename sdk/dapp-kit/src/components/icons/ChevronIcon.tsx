// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ComponentProps } from 'react';

export function ChevronIcon(props: ComponentProps<'svg'>) {
	return (
		<svg xmlns="http://www.w3.org/2000/svg" width={16} height={16} fill="none" {...props}>
			<path
				stroke="#A0B6C3"
				strokeLinecap="round"
				strokeLinejoin="round"
				strokeWidth={1.5}
				d="m4 6 4 4 4-4"
			/>
		</svg>
	);
}
