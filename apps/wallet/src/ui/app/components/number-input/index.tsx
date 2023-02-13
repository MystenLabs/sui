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
}

function NumberInput<FormValues>({
    allowNegative,
    className,
    placeholder,
    prefix,
    suffix,
    disabled: forcedDisabled,
    decimals = false,
    field: { onBlur, name, value },
    form: { isSubmitting, setFieldValue },
}: NumberInputProps<FormValues>) {
    const disabled =
        forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
    return (
        <NumericFormat
            valueIsNumericString
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
                prefix,
                suffix,
                onValueChange: (values) => setFieldValue(name, values.value),
            }}
        />
    );
}

export default NumberInput;
