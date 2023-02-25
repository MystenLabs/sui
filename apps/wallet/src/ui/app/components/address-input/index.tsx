// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useField } from 'formik';
import { useCallback, useMemo } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { SUI_ADDRESS_VALIDATION } from './validation';
import { Text } from '_app/shared/text';
import Alert from '_src/ui/app/components/alert';

import type { SuiAddress } from '@mysten/sui.js';
import type { FieldProps } from 'formik';
import type { ChangeEventHandler } from 'react';

export interface AddressInputProps<Values>
    extends FieldProps<SuiAddress, Values> {
    disabled?: boolean;
    placeholder?: string;
    className?: string;
}

export function AddressInput<FormValues>({
    disabled: forcedDisabled,
    placeholder = '0x...',
    form: { isSubmitting, dirty, setFieldValue, isValid },
    field: { onBlur, name, value },
}: AddressInputProps<FormValues>) {
    const disabled =
        forcedDisabled !== undefined ? forcedDisabled : isSubmitting;
    const handleOnChange = useCallback<ChangeEventHandler<HTMLTextAreaElement>>(
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

    const clearAddress = useCallback(() => {
        setFieldValue('to', '');
    }, [setFieldValue]);

    const [, { touched, error }] = useField(name);

    return (
        <>
            <div
                className={cx(
                    'flex h-11 py-1 w-full px-3 pr-0 items-center rounded-2lg bg-white border border-solid box-border focus-within:border-steel transition-all overflow-hidden',
                    touched && error ? 'border-issue' : 'border-gray-45'
                )}
            >
                <TextareaAutosize
                    maxRows={2}
                    minRows={1}
                    disabled={disabled}
                    placeholder={placeholder}
                    value={formattedValue}
                    onChange={handleOnChange}
                    onBlur={onBlur}
                    className={cx(
                        'w-full text-bodySmall leading-100 font-medium font-mono bg-white placeholder:text-steel-dark placeholder:font-normal placeholder:font-mono border-none resize-none',
                        touched && error ? 'text-issue' : 'text-gray-90'
                    )}
                    name={name}
                />

                <div
                    onClick={clearAddress}
                    className={cx(
                        'flex bg-gray-40 items-center justify-center h-10 w-10 p-0.5 mr-0 right-0 max-w-[20%] mx-3 overflow-hidden',
                        touched
                            ? 'cursor-pointer text-steel-darker text-body font-medium sui-icons-close'
                            : "bg-[url('_assets/images/qr-code.svg')] bg-no-repeat bg-center pr-0"
                    )}
                ></div>
            </div>

            {touched ? (
                <div className="mt-3 w-full">
                    <Alert mode={error ? 'warning' : 'success'}>
                        <Text variant="bodySmall" weight="medium">
                            {error ? error : 'Valid address'}
                        </Text>
                    </Alert>
                </div>
            ) : null}
        </>
    );
}
