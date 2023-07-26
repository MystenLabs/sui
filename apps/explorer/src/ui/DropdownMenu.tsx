// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text, Toggle } from '@mysten/ui';
import * as RadixDropdownMenu from '@radix-ui/react-dropdown-menu';
import { type ReactNode } from 'react';

export type DropdownMenuProps = {
	content: ReactNode;
	trigger: ReactNode;
	side?: RadixDropdownMenu.MenuContentProps['side'];
	align?: RadixDropdownMenu.MenuContentProps['align'];
} & Omit<RadixDropdownMenu.DropdownMenuProps, 'className' | 'asChild'>;

export function DropdownMenu({
	content,
	side,
	trigger,
	align,
	...radixRootProps
}: DropdownMenuProps) {
	return (
		<RadixDropdownMenu.Root {...radixRootProps}>
			<RadixDropdownMenu.Trigger className="text-steel hover:text-steel-dark data-[state=open]:text-steel-dark">
				{trigger}
			</RadixDropdownMenu.Trigger>
			<RadixDropdownMenu.Portal>
				<RadixDropdownMenu.Content
					side={side}
					align={align}
					className="z-10 min-w-[280px] rounded-md bg-white p-1 shadow-mistyEdge"
				>
					{content}
				</RadixDropdownMenu.Content>
			</RadixDropdownMenu.Portal>
		</RadixDropdownMenu.Root>
	);
}

export type DropdownMenuCheckboxItemProps = Omit<
	RadixDropdownMenu.DropdownMenuCheckboxItemProps,
	'className' | 'checked' | 'asChild'
> & { checked?: boolean; label: ReactNode };
export function DropdownMenuCheckboxItem({
	checked = false,
	...radixRootProps
}: DropdownMenuCheckboxItemProps) {
	return (
		<RadixDropdownMenu.CheckboxItem {...radixRootProps} asChild>
			<div className="flex cursor-pointer select-none items-center gap-4 rounded-md p-2 text-steel-dark outline-none transition-colors data-[highlighted]:bg-sui-light/50 data-[highlighted]:text-steel-darker">
				<div className="flex-1">
					<Text variant="body/medium">Show System Transactions</Text>
				</div>
				<Toggle
					onClick={(e) => {
						e.stopPropagation();
					}}
					checked={checked}
					/* eslint-disable-next-line react/jsx-handler-names */
					onCheckedChange={radixRootProps.onCheckedChange}
				/>
			</div>
		</RadixDropdownMenu.CheckboxItem>
	);
}
