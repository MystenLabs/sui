// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';
import type { ComponentProps } from 'react';

type TextAreaProps = Omit<ComponentProps<'textarea'>, 'className' | 'ref'>;

export const TextArea = forwardRef<HTMLTextAreaElement, TextAreaProps>((props, forwardedRef) => (
	<textarea
		className="resize-none w-full text-body text-steel-dark font-medium p-3 border border-solid border-gray-45 rounded-2lg shadow-button focus:border-steel focus:shadow-none"
		ref={forwardedRef}
		{...props}
	/>
));
