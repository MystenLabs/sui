// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight16 } from '@mysten/icons';
import { Heading } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import { Card, type CardProps } from '~/ui/Card';

type Size = 'md' | 'sm';

interface CollapsibleCardHeaderProps {
	open: boolean;
	size: Size;
	title?: string | ReactNode;
	collapsible?: boolean;
}

function CollapsibleCardHeader({ open, size, title, collapsible }: CollapsibleCardHeaderProps) {
	if (!title) {
		return null;
	}

	const headerContent = (
		<div
			className={clsx(
				'flex w-full justify-between',
				size === 'md' ? 'px-6' : 'px-4',
				size === 'sm' && 'pb-4.5',
				open && size === 'md' && 'pb-6 pt-7',
				open && size === 'sm' && 'pt-4.5',
				!open && size === 'md' && 'py-7',
				!open && size === 'sm' && 'py-4.5',
			)}
		>
			{typeof title === 'string' ? (
				<Heading
					variant={size === 'md' ? 'heading4/semibold' : 'heading6/semibold'}
					color="steel-darker"
				>
					{title}
				</Heading>
			) : (
				title
			)}

			{collapsible && (
				<ChevronRight16 className={clsx('cursor-pointer text-steel', open && 'rotate-90')} />
			)}
		</div>
	);

	if (collapsible) {
		return (
			<Collapsible.Trigger asChild>
				<div className="cursor-pointer">{headerContent}</div>
			</Collapsible.Trigger>
		);
	}

	return <>{headerContent}</>;
}

export interface CollapsibleCardProps extends Omit<CardProps, 'size'> {
	children: ReactNode;
	title?: string | ReactNode;
	footer?: ReactNode;
	collapsible?: boolean;
	size?: Size;
	initialClose?: boolean;
	growOnHover?: boolean;
}

export function CollapsibleCard({
	title,
	footer,
	collapsible,
	size = 'md',
	children,
	initialClose,
	growOnHover,
	...cardProps
}: CollapsibleCardProps) {
	const [open, setOpen] = useState(!initialClose);
	return (
		<div className="relative w-full">
			<Card
				rounded="2xl"
				border="gray45"
				bg="white"
				spacing="none"
				growOnHover={growOnHover && !open}
				{...cardProps}
			>
				<Collapsible.Root
					open={open}
					onOpenChange={setOpen}
					className={clsx(
						!title && size === 'md' && 'pt-7',
						!title && size === 'sm' && 'pt-4.5',
						open && size === 'md' && 'pb-7',
						open && size === 'sm' && 'pb-4.5',
					)}
				>
					<CollapsibleCardHeader open={open} size={size} title={title} collapsible={collapsible} />

					<div className={clsx(size === 'md' ? 'px-6' : 'px-4')}>
						<Collapsible.Content>{children}</Collapsible.Content>
					</div>
				</Collapsible.Root>

				{footer && (
					<div className={clsx('rounded-b-2xl bg-sui/10 py-2.5', size === 'md' ? 'px-6' : 'px-4')}>
						{footer}
					</div>
				)}
			</Card>
		</div>
	);
}
