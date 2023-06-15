// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { NumericFormat } from 'react-number-format';

import type { FieldProps } from 'formik';

export interface NumberInputProps<Values> extends FieldProps<string, Values> {
	allowNegative: boolean;
	className?: string;
	placeholder?: string;
	disabled?: boolean;
	decimals?: boolean;
	suffix?: string;
	prefix?: string;
	autoFocus?: boolean;
	'data-testid'?: string;
}

function NumberInput<FormValues>({
	allowNegative,
	className,
	placeholder,
	disabled: forcedDisabled,
	decimals = false,
	field: { onBlur, name, value },
	form: { isSubmitting, setFieldValue },
	prefix,
	suffix,
	autoFocus,
	'data-testid': testId,
}: NumberInputProps<FormValues>) {
	const disabled = forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
	return (
		<NumericFormat
			valueIsNumericString
			data-testid={testId}
			{...{
				className,
				placeholder,
				disabled,
				value,
				name,
				allowNegative,
				decimalScale: decimals ? undefined : 0,
				thousandSeparator: true,
				onBlur,
				onValueChange: (values) => setFieldValue(name, values.value),
				prefix,
				suffix,
				autoFocus,
			}}
		/>
	);
}

export default NumberInput;
