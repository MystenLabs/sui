// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronDown12 } from '@mysten/icons';
import * as SelectPrimitive from '@radix-ui/react-select';
import { forwardRef } from 'react';
import { Text } from '_app/shared/text';

const Select = SelectPrimitive.Root;
const SelectValue = SelectPrimitive.Value;

const SelectTrigger = forwardRef<
	React.ElementRef<typeof SelectPrimitive.Trigger>,
	React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
	<SelectPrimitive.Trigger
		ref={ref}
		className="flex items-center border border-solid border-gray-45 shadow-sm rounded-2lg bg-white px-4 py-2 gap-1.5 focus:outline-none h-[40px] cursor-pointer disabled:cursor-default"
		{...props}
	>
		{children}
		<SelectPrimitive.Icon asChild>
			<ChevronDown12 className="text-steel" />
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
			<SelectPrimitive.Viewport className="bg-white p-2 border border-solid border-gray-45 rounded-md shadow-md">
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
		className="flex items-center hover:border-none hover:outline-none hover:cursor-pointer w-full hover:bg-hero-darkest hover:bg-opacity-5 p-2 rounded-sm"
		{...props}
	>
		<SelectPrimitive.ItemText>
			<Text variant="body" weight="semibold" color="steel">
				{children}
			</Text>
		</SelectPrimitive.ItemText>
	</SelectPrimitive.Item>
));
SelectItem.displayName = SelectPrimitive.Item.displayName;

export { Select, SelectTrigger, SelectContent, SelectItem, SelectValue };
