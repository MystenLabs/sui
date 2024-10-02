// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ComponentProps } from 'react';

// FIXME: Replace this with a 10x10 icon to match the CheckIcon, or alternatively make the CheckIcon bigger
// Right now, the icons don't align on mobile :(
export function BackIcon(props: ComponentProps<'svg'>) {
	return (
		<svg width={24} height={24} fill="none" xmlns="http://www.w3.org/2000/svg" {...props}>
			<path
				d="M7.57 12.262c0 .341.13.629.403.895l5.175 5.059c.204.205.45.307.751.307.609 0 1.101-.485 1.101-1.087 0-.293-.123-.574-.349-.8L10.14 12.27l4.511-4.375A1.13 1.13 0 0 0 15 7.087C15 6.485 14.508 6 13.9 6c-.295 0-.54.103-.752.308l-5.175 5.058c-.28.28-.404.56-.404.896Z"
				fill="currentColor"
			/>
		</svg>
	);
}
