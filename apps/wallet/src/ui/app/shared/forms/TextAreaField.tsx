// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ComponentProps, type ReactNode } from 'react';

import { TextArea } from './controls/TextArea';
import FormField from './FormField';

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
