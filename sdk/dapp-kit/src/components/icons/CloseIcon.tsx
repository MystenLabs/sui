// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ComponentProps } from 'react';

export function CloseIcon(props: ComponentProps<'svg'>) {
	return (
		<svg width={10} height={10} fill="none" xmlns="http://www.w3.org/2000/svg" {...props}>
			<path
				d="M9.708.292a.999.999 0 0 0-1.413 0l-3.289 3.29L1.717.291A.999.999 0 0 0 .305 1.705l3.289 3.289-3.29 3.289a.999.999 0 1 0 1.413 1.412l3.29-3.289 3.288 3.29a.999.999 0 0 0 1.413-1.413l-3.29-3.29 3.29-3.288a.999.999 0 0 0 0-1.413Z"
				fill="currentColor"
			/>
		</svg>
	);
}
