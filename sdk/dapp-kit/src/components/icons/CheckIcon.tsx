// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ComponentProps } from 'react';

export function CheckIcon(props: ComponentProps<'svg'>) {
	return (
		<svg xmlns="http://www.w3.org/2000/svg" width={16} height={16} fill="none" {...props}>
			<path
				fill="currentColor"
				d="m11.726 5.048-4.73 5.156-1.722-1.879a.72.72 0 0 0-.529-.23.722.722 0 0 0-.525.24.858.858 0 0 0-.22.573.86.86 0 0 0 .211.576l2.255 2.458c.14.153.332.24.53.24.2 0 .391-.087.532-.24l5.261-5.735A.86.86 0 0 0 13 5.63a.858.858 0 0 0-.22-.572.722.722 0 0 0-.525-.24.72.72 0 0 0-.529.23Z"
			/>
		</svg>
	);
}
