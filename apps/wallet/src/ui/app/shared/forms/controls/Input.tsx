// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';
import type { ComponentProps } from 'react';

type InputProps = Omit<ComponentProps<'input'>, 'className'>;

export const Input = forwardRef<HTMLInputElement, InputProps>((props, forwardedRef) => (
	<input
		className="transition peer items-center p-3 bg-white text-body font-medium placeholder:text-gray-60 w-full shadow-sm border-solid border border-gray-45 text-steel-dark hover:text-steel-darker focus:text-steel-darker rounded-lg hover:border-steel focus:border-steel disabled:border-gray-45 disabled:text-gray-60"
		ref={forwardedRef}
		{...props}
	/>
));
