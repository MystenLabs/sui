// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';
import { ComponentPropsWithoutRef, ElementRef, forwardRef } from 'react';

export const RadioGroup = forwardRef<
	ElementRef<typeof RadioGroupPrimitive.Root>,
	Omit<ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Root>, 'className'> & {
		'aria-label': string;
	}
>(({ ...props }, ref) => {
	return <RadioGroupPrimitive.Root className="flex gap-0.5" {...props} ref={ref} />;
});

export const RadioGroupItem = forwardRef<
	ElementRef<typeof RadioGroupPrimitive.Item>,
	ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item> & { label: string }
>(({ label, ...props }, ref) => {
	return (
		<RadioGroupPrimitive.Item
			ref={ref}
			className="flex flex-col rounded-md border border-transparent bg-white text-steel-dark hover:text-steel-darker data-[state=checked]:border-steel  data-[state=checked]:text-hero-dark  disabled:text-gray-60 px-2 py-1 text-captionSmall font-semibold"
			{...props}
		>
			{label}
		</RadioGroupPrimitive.Item>
	);
});
