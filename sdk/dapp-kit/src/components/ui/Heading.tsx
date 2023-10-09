// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Slot } from '@radix-ui/react-slot';
import clsx from 'clsx';
import { forwardRef } from 'react';

import { headingVariants } from './Heading.css.js';
import type { HeadingVariants } from './Heading.css.js';

type HeadingAsChildProps = {
	asChild?: boolean;
	as?: never;
};

type HeadingAsProps = {
	as?: 'h1' | 'h2' | 'h3' | 'h4' | 'h5' | 'h6';
	asChild?: never;
};

type HeadingProps = (HeadingAsChildProps | HeadingAsProps) &
	React.HTMLAttributes<HTMLHeadingElement> &
	HeadingVariants;

const Heading = forwardRef<HTMLHeadingElement, HeadingProps>(
	(
		{
			children,
			className,
			asChild = false,
			as: Tag = 'h1',
			size,
			weight,
			truncate,
			...headingProps
		},
		forwardedRef,
	) => {
		return (
			<Slot
				{...headingProps}
				ref={forwardedRef}
				className={clsx(headingVariants({ size, weight, truncate }), className)}
			>
				{asChild ? children : <Tag>{children}</Tag>}
			</Slot>
		);
	},
);
Heading.displayName = 'Heading';

export { Heading };
