// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ReactNode } from 'react';
import { Controller, useFormContext } from 'react-hook-form';

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './controls/Select';

type SelectFieldProps = {
	name: string;
	options: string[] | { id: string; label: ReactNode }[];
	disabled?: boolean;
};

export const SelectField = forwardRef<HTMLButtonElement, SelectFieldProps>(
	({ name, options, ...props }, forwardedRef) => {
		const { control } = useFormContext();
		return (
			<Controller
				control={control}
				name={name}
				render={({ field }) => (
					<Select onValueChange={field.onChange} value={field.value} {...props}>
						<SelectTrigger ref={forwardedRef}>
							<SelectValue />
						</SelectTrigger>
						<SelectContent position="popper" align="end">
							{options.map((option, index) => (
								<SelectItem value={typeof option === 'string' ? option : option.id} key={index}>
									{typeof option === 'string' ? option : option.label}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				)}
			/>
		);
	},
);
