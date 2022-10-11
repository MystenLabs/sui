// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useCallback } from 'react';
import NumberFormat from 'react-number-format';

import { useNumberDelimiters } from '_hooks';

import type { FieldProps } from 'formik';
import type { NumberFormatValues } from 'react-number-format';

export interface NumberInputProps<Values> extends FieldProps<string, Values> {
    allowNegative: boolean;
    className?: string;
    placeholder?: string;
    disabled?: boolean;
    decimals?: boolean;
}

function NumberInput<FormValues>({
    allowNegative,
    className,
    placeholder,
    disabled: forcedDisabled,
    decimals = false,
    field: { onBlur, name, value },
    form: { isSubmitting, setFieldValue },
}: NumberInputProps<FormValues>) {
    const disabled =
        forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
    const { groupDelimiter, decimalDelimiter } = useNumberDelimiters();
    const handleOnValueChange = useCallback(
        (values: NumberFormatValues) => {
            setFieldValue(name, values.value);
        },
        [name, setFieldValue]
    );
    return (
        <NumberFormat
            type="text"
            {...{
                className,
                placeholder,
                disabled,
                value,
                name,
                allowNegative,
                decimalScale: decimals ? undefined : 0,
                decimalSeparator: decimalDelimiter || '.',
                thousandSeparator: groupDelimiter || ',',
                onBlur,
                onValueChange: handleOnValueChange,
            }}
        />
    );
}

export default memo(NumberInput);
