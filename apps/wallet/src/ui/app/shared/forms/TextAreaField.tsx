// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, type ReactNode, forwardRef } from 'react';
import FormField from './FormField';
import { TextArea } from './controls/TextArea';

type TextAreaFieldProps = {
	name: string;
	label: ReactNode;
} & ComponentProps<'textarea'>;

export const TextAreaField = forwardRef<HTMLTextAreaElement, TextAreaFieldProps>(
	({ label, ...props }, forwardedRef) => (
		<FormField name={props.name} label={label}>
			<TextArea {...props} ref={forwardedRef} />
		</FormField>
	),
);
