// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Slot } from '@radix-ui/react-slot';
import type { ButtonHTMLAttributes, ReactNode } from 'react';

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
	children: ReactNode;
	'aria-label': string;
	asChild?: boolean;
}

export function IconButton({ asChild, children, ...props }: IconButtonProps) {
	const Comp = asChild ? Slot : 'button';
	return (
		<Comp
			className="inline-flex cursor-pointer items-center justify-center bg-transparent px-3 py-2 text-steel-dark hover:text-steel-darker active:text-steel disabled:cursor-default disabled:text-gray-60"
			{...props}
		>
			{children}
		</Comp>
	);
}
