// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EyeClose16, EyeOpen16 } from '@mysten/icons';
import { useField } from 'formik';
import { useState, type ComponentProps } from 'react';

export interface PasswordInputProps
	extends Omit<ComponentProps<'input'>, 'className' | 'type' | 'name'> {
	name: string;
}

export function PasswordInputField({ ...props }: PasswordInputProps) {
	const [passwordShown, setPasswordShown] = useState(false);
	const [field] = useField(props.name);
	const IconComponent = passwordShown ? EyeOpen16 : EyeClose16;
	return (
		<div className="flex w-full relative items-center">
			<input
				type={passwordShown ? 'text' : 'password'}
				placeholder="Password"
				{...props}
				{...field}
				className={
					'peer h-11 w-full text-body text-steel-dark font-medium flex items-center gap-5 bg-white py-2.5 pr-0 pl-3 border border-solid  border-gray-45 rounded-2lg shadow-button focus:border-steel focus:shadow-none placeholder-gray-65'
				}
			/>
			<IconComponent
				className="absolute text-heading6 font-normal text-gray-60 cursor-pointer right-3 peer-focus:text-steel"
				onClick={() => setPasswordShown(!passwordShown)}
			/>
		</div>
	);
}
