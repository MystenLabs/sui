// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight12, ChevronRight16 } from '@mysten/icons';
import { Heading, Text } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { useState, type ReactNode } from 'react';

import { Card, type CardProps } from '~/ui/Card';
import { Divider } from '~/ui/Divider';

type Size = 'md' | 'sm';

interface TransactionBlockCardHeaderProps {
	open: boolean;
	size: Size;
	title?: string | ReactNode;
	collapsible?: boolean;
}

function TransactionBlockCardHeader({
	open,
	size,
	title,
	collapsible,
}: TransactionBlockCardHeaderProps) {
	if (!title) {
		return null;
	}

	const headerContent = (
		<div
			className={clsx(
				'flex w-full justify-between',
				open && size === 'md' && 'pb-6',
				open && size === 'sm' && 'pb-4.5',
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

interface TransactionBlockCardSectionProps {
	children: ReactNode;
	defaultOpen?: boolean;
	title?: string | ReactNode;
}

export function TransactionBlockCardSection({
	title,
	defaultOpen = true,
	children,
}: TransactionBlockCardSectionProps) {
	const [open, setOpen] = useState(defaultOpen);
	return (
		<Collapsible.Root open={open} onOpenChange={setOpen} className="flex w-full flex-col gap-3">
			{title && (
				<Collapsible.Trigger>
					<div className="flex items-center gap-2">
						{typeof title === 'string' ? (
							<Text color="steel-darker" variant="body/semibold">
								{title}
							</Text>
						) : (
							title
						)}
						<Divider />
						<ChevronRight12
							className={clsx('h-4 w-4 cursor-pointer text-gray-45', open && 'rotate-90')}
						/>
					</div>
				</Collapsible.Trigger>
			)}

			<Collapsible.Content>{children}</Collapsible.Content>
		</Collapsible.Root>
	);
}

export interface TransactionBlockCardProps extends Omit<CardProps, 'size'> {
	children: ReactNode;
	title?: string | ReactNode;
	footer?: ReactNode;
	collapsible?: boolean;
	size?: Size;
}

export function TransactionBlockCard({
	title,
	footer,
	collapsible,
	size = 'md',
	children,
	...cardProps
}: TransactionBlockCardProps) {
	const [open, setOpen] = useState(true);
	return (
		<div className="relative w-full">
			<Card rounded="2xl" border="gray45" bg="white" spacing="none" {...cardProps}>
				<Collapsible.Root
					open={open}
					onOpenChange={setOpen}
					className={clsx(size === 'md' ? 'px-6 py-7' : 'px-4 py-4.5')}
				>
					<TransactionBlockCardHeader
						open={open}
						size={size}
						title={title}
						collapsible={collapsible}
					/>

					<Collapsible.Content>{children}</Collapsible.Content>
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
