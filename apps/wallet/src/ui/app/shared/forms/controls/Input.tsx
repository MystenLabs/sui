// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';
import type { ComponentProps } from 'react';

type InputProps = Omit<ComponentProps<'input'>, 'className'>;

export const Input = forwardRef<HTMLInputElement, InputProps>((props, forwardedRef) => (
	<input
		className="transition peer items-center p-3 bg-white text-body font-medium placeholder:text-gray-60 w-full shadow-button border-solid border border-gray-45 text-steel-darker rounded-2lg hover:border-steel focus:border-steel disabled:border-gray-40 disabled:text-gray-55"
		ref={forwardedRef}
		{...props}
	/>
));
