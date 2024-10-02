// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import { ChevronDown12 } from '@mysten/icons';
import * as SelectPrimitive from '@radix-ui/react-select';
import { forwardRef } from 'react';

const Select = SelectPrimitive.Root;
const SelectValue = SelectPrimitive.Value;

const SelectTrigger = forwardRef<
	React.ElementRef<typeof SelectPrimitive.Trigger>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Trigger
		ref={ref}
		className="flex transition items-center text-steel-dark hover:text-steel-darker active:text-steel-dark disabled:text-gray-60 border border-solid border-gray-45 hover:border-steel disabled:border-gray-45 shadow-sm rounded-lg bg-white px-4 py-3 gap-0.5 focus:outline-none cursor-pointer disabled:cursor-default group active:bg-hero/5 disabled:bg-white"
		{...props}
	>
		{children}
		<SelectPrimitive.Icon asChild>
			<ChevronDown12 className="transition text-steel group-hover:text-steel-darker group-active:text-steel-dark group-disabled:text-gray-45" />
		</SelectPrimitive.Icon>
	</SelectPrimitive.Trigger>
));
SelectTrigger.displayName = SelectPrimitive.Trigger.displayName;

const SelectContent = forwardRef<
	React.ElementRef<typeof SelectPrimitive.Content>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Portal>
		<SelectPrimitive.Content
			ref={ref}
			className="z-[99999] min-w-[112px] bg-transparent"
			{...props}
		>
			<SelectPrimitive.Viewport className="bg-white p-2 border border-solid border-gray-45 rounded-lg shadow-sm">
				{children}
			</SelectPrimitive.Viewport>
		</SelectPrimitive.Content>
	</SelectPrimitive.Portal>
));
SelectContent.displayName = SelectPrimitive.Content.displayName;

const SelectItem = forwardRef<
	React.ElementRef<typeof SelectPrimitive.Item>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Item
		ref={ref}
		className="transition flex items-center text-steel-dark cursor-pointer p-2 outline-none rounded-md hover:text-steel-darker hover:bg-hero/5"
		{...props}
	>
		<SelectPrimitive.ItemText>
			<Text variant="body" weight="semibold">
				{children}
			</Text>
		</SelectPrimitive.ItemText>
	</SelectPrimitive.Item>
));
SelectItem.displayName = SelectPrimitive.Item.displayName;

export { Select, SelectTrigger, SelectContent, SelectItem, SelectValue };
