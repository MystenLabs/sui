// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';

import { Label } from './utils/Label';

import type { ComponentProps } from 'react';

export interface InputProps extends Omit<ComponentProps<'input'>, 'ref' | 'className'> {
	label: string;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(({ label, ...inputProps }, ref) => (
	<Label label={label}>
		<input
			ref={ref}
			{...inputProps}
			className="rounded-md border border-gray-45 bg-white p-2 text-body font-medium text-steel-darker shadow-sm shadow-ebony/10 placeholder:text-gray-60"
		/>
	</Label>
));
