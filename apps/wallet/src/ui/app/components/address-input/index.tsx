// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X12, QrCode } from '@mysten/icons';
import { cx } from 'class-variance-authority';
import { useField, useFormikContext } from 'formik';
import { useCallback, useMemo } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { SUI_ADDRESS_VALIDATION } from './validation';
import { Text } from '_app/shared/text';
import Alert from '_src/ui/app/components/alert';

import type { ChangeEventHandler } from 'react';

export interface AddressInputProps {
    disabled?: boolean;
    placeholder?: string;
    name: string;
}

export function AddressInput({
    disabled: forcedDisabled,
    placeholder = '0x...',
    name = 'to',
}: AddressInputProps) {
    const [field, meta] = useField(name);

    const { isSubmitting, setFieldValue } = useFormikContext();

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
        () => SUI_ADDRESS_VALIDATION.cast(field?.value),
        [field?.value]
    );

    const clearAddress = useCallback(() => {
        setFieldValue('to', '');
    }, [setFieldValue]);

    return (
        <>
            <div
                className={cx(
                    'flex h-max w-full rounded-2lg bg-white border border-solid box-border focus-within:border-steel transition-all overflow-hidden',
                    meta.touched && meta.error
                        ? 'border-issue'
                        : 'border-gray-45'
                )}
            >
                <div className="min-h-[42px] w-full flex items-center pl-3 py-1">
                    <TextareaAutosize
                        maxRows={3}
                        minRows={1}
                        disabled={disabled}
                        placeholder={placeholder}
                        value={formattedValue}
                        onChange={handleOnChange}
                        onBlur={field.onBlur}
                        className={cx(
                            'w-full text-bodySmall leading-100 font-medium font-mono bg-white placeholder:text-steel-dark placeholder:font-normal placeholder:font-mono border-none resize-none',
                            meta.touched && meta.error
                                ? 'text-issue'
                                : 'text-gray-90'
                        )}
                        name={name}
                    />
                </div>

                <div
                    onClick={clearAddress}
                    className="flex bg-gray-40 items-center justify-center w-12 p-0.5 mr-0 right-0 max-w-[20%] mx-3.5 cursor-pointer"
                >
                    {meta.touched && field.value ? (
                        <X12 className="h-3 w-3 text-steel-darker" />
                    ) : (
                        <QrCode className="h-5 w-5 text-steel-darker" />
                    )}
                </div>
            </div>

            {meta.touched ? (
                <div className="mt-3 w-full">
                    <Alert mode={meta.error ? 'warning' : 'success'}>
                        <Text variant="bodySmall" weight="medium">
                            {meta.error ? meta.error : 'Valid address'}
                        </Text>
                    </Alert>
                </div>
            ) : null}
        </>
    );
}
