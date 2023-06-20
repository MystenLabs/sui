// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from 'clsx';
import { type ReactNode } from 'react';

export function HighlightedTableCol({ children, first }: { children: ReactNode; first?: boolean }) {
	return (
		<div
			className={clsx(
				'mr-3 flex h-full items-center rounded hover:bg-sui-light',
				!first && '-ml-3',
			)}
		>
			<div className={clsx(!first && 'ml-3')}>{children}</div>
		</div>
	);
}
