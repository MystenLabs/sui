// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronDown12, ChevronRight12 } from '@mysten/icons';

import * as CollapsiblePrimitive from '@radix-ui/react-collapsible';
import { useState, type ReactNode } from 'react';

type CollapsibleProps = {
	title: string;
	defaultOpen?: boolean;
	children: ReactNode | ReactNode[];
};

export function Collapsible({ title, children, defaultOpen }: CollapsibleProps) {
	const [open, setOpen] = useState(defaultOpen ?? false);
	return (
		<CollapsiblePrimitive.Root
			className="flex flex-shrink-0 justify-start flex-col w-full gap-3"
			open={open}
			onOpenChange={setOpen}
		>
			<CollapsiblePrimitive.Trigger className="flex items-center gap-2 w-full bg-transparent border-none p-0 cursor-pointer group">
				<div className="text-captionSmall font-semibold uppercase text-steel group-hover:text-hero">
					{title}
				</div>
				<div className="h-px bg-steel group-hover:bg-hero flex-1" />
				<div className="text-steel group-hover:text-hero inline-flex">
					{open ? <ChevronDown12 /> : <ChevronRight12 />}
				</div>
			</CollapsiblePrimitive.Trigger>

			<CollapsiblePrimitive.Content>{children}</CollapsiblePrimitive.Content>
		</CollapsiblePrimitive.Root>
	);
}
