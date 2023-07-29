// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ComponentProps, type ReactNode } from 'react';

export interface NavItemProps extends ComponentProps<'button'> {
	beforeIcon?: ReactNode;
	afterIcon?: ReactNode;
	children: ReactNode;
}

export const NavItem = forwardRef<HTMLButtonElement, NavItemProps>(
	({ children, beforeIcon, afterIcon, ...props }, ref) => (
		<button
			ref={ref}
			type="button"
			className="flex cursor-pointer items-center gap-1 rounded-md border-none bg-transparent px-3.5 py-2 text-heading6 font-medium text-hero-darkest outline-none hover:bg-white/60 ui-open:bg-white/60"
			{...props}
		>
			{beforeIcon}
			{children}
			{afterIcon}
		</button>
	),
);
