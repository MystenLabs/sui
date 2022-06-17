// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { memo, useCallback, useMemo } from 'react';

import { SUI_ADDRESS_VALIDATION } from './validation';

import type { SuiAddress } from '@mysten/sui.js';
import type { FieldProps } from 'formik';
import type { ChangeEventHandler } from 'react';

export interface AddressInputProps<Values>
    extends FieldProps<SuiAddress, Values> {
    disabled?: boolean;
    placeholder?: string;
    className?: string;
}

function AddressInput<FormValues>({
    disabled: forcedDisabled,
    placeholder = '0x...',
    className,
    form: { isSubmitting, setFieldValue },
    field: { onBlur, name, value },
}: AddressInputProps<FormValues>) {
    const disabled =
        forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
    const handleOnChange = useCallback<ChangeEventHandler<HTMLInputElement>>(
        (e) => {
            const address = e.currentTarget.value;
            setFieldValue(name, SUI_ADDRESS_VALIDATION.cast(address));
        },
        [setFieldValue, name]
    );
    const formattedValue = useMemo(
        () => SUI_ADDRESS_VALIDATION.cast(value),
        [value]
    );
    return (
        <input
            type="text"
            {...{
                disabled,
                placeholder,
                className,
                onBlur,
                value: formattedValue,
                name,
                onChange: handleOnChange,
            }}
        />
    );
}

export default memo(AddressInput);
