// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as RadixDialog from '@radix-ui/react-dialog';
import { cx } from 'class-variance-authority';
import * as React from 'react';
import { Heading } from './heading';
import { Text } from './text';

export const Dialog = RadixDialog.Root;
export const DialogTrigger = RadixDialog.Trigger;

const DialogPortal = ({ className, ...props }: RadixDialog.DialogPortalProps) => (
	<RadixDialog.Portal className={cx(className)} {...props} />
);

const DialogOverlay = React.forwardRef<
	React.ElementRef<typeof AnimatedOverlay>,
	React.ComponentPropsWithoutRef<typeof AnimatedOverlay>
>(({ className, ...props }, ref) => (
	<RadixDialog.Overlay
		ref={ref}
		className={cx(
			'bg-gray-95/10 backdrop-blur-lg z-[99998] fixed inset-0 bg-background/80',
			className,
		)}
		{...props}
	/>
));

export const DialogContent = React.forwardRef<
	React.ElementRef<typeof AnimatedContent>,
	React.ComponentPropsWithoutRef<typeof AnimatedContent>
>(({ className, children, ...props }, ref) => (
	<DialogPortal>
		<DialogOverlay />
		<RadixDialog.Content
			ref={ref}
			className={cx(
				'fixed flex flex-col items-center justify-center z-[99999] left-[50%] top-[50%] translate-x-[-50%] translate-y-[-50%] shadow-wallet-modal bg-white p-6 rounded-xl w-80 max-w-[85vw] max-h-[60vh] overflow-hidden gap-1.5',
				className,
			)}
			{...props}
		>
			{children}
		</RadixDialog.Content>
	</DialogPortal>
));

export const DialogTitle = React.forwardRef<
	React.ElementRef<typeof RadixDialog.Title>,
	Omit<React.ComponentPropsWithoutRef<typeof RadixDialog.Title>, 'asChild' | 'className'>
>(({ children, ...props }, ref) => (
	<RadixDialog.Title ref={ref} asChild {...props} id="WTF">
		<Heading variant="heading6" weight="semibold" color="gray-90" as="h2">
			{children}
		</Heading>
	</RadixDialog.Title>
));

export const DialogDescription = React.forwardRef<
	React.ElementRef<typeof RadixDialog.Description>,
	Omit<React.ComponentPropsWithoutRef<typeof RadixDialog.Description>, 'asChild' | 'className'>
>(({ children, ...props }, ref) => (
	<RadixDialog.Description ref={ref} asChild {...props}>
		<Text variant="pBodySmall" color="steel">
			{children}
		</Text>
	</RadixDialog.Description>
));
