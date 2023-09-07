// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';
import { Controller, useFormContext } from 'react-hook-form';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './controls/Select';

type SelectFieldProps = {
	name: string;
	options: string[];
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
					<Select onValueChange={field.onChange} defaultValue={field.value} {...props}>
						<SelectTrigger ref={forwardedRef}>
							<SelectValue />
						</SelectTrigger>
						<SelectContent position="popper" sideOffset={-41} align="end">
							{options.map((option, index) => (
								<SelectItem value={option} key={index}>
									{option}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				)}
			/>
		);
	},
);
