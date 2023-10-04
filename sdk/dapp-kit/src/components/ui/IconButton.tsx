// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Slot } from '@radix-ui/react-slot';
import clsx from 'clsx';
import type { ButtonHTMLAttributes } from 'react';
import { forwardRef } from 'react';

import * as styles from './IconButton.css.js';

type IconButtonProps = {
	asChild?: boolean;
	'aria-label': string;
} & ButtonHTMLAttributes<HTMLButtonElement>;

const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
	({ className, asChild = false, ...props }, forwardedRef) => {
		const Comp = asChild ? Slot : 'button';
		return <Comp {...props} className={clsx(styles.container, className)} ref={forwardedRef} />;
	},
);
IconButton.displayName = 'Button';

export { IconButton };
