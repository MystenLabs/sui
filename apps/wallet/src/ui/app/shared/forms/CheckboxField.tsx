// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ComponentProps, type ReactNode } from 'react';
import { Controller, useFormContext } from 'react-hook-form';

import { Checkbox } from './controls/Checkbox';
import FormField from './FormField';

type CheckboxFieldProps = {
	name: string;
	label: ReactNode;
} & Omit<ComponentProps<'button'>, 'ref'>;

export const CheckboxField = forwardRef<HTMLButtonElement, CheckboxFieldProps>(
	({ label, name, ...props }, forwardedRef) => {
		const { control } = useFormContext();
		return (
			<Controller
				control={control}
				name={name}
				render={({ field: { onChange, name, value } }) => (
					<FormField name={name}>
						<Checkbox
							label={label}
							onCheckedChange={onChange}
							name={name}
							checked={value}
							ref={forwardedRef}
							{...props}
						/>
					</FormField>
				)}
			/>
		);
	},
);
