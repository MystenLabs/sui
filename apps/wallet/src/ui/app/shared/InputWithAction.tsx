// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useField, useFormikContext } from 'formik';
import { NumericFormat } from 'react-number-format';

import Alert from '../components/alert';
import { Pill, type PillProps } from './Pill';

import type { ComponentProps } from 'react';

export interface InputWithActionProps
    extends Omit<ComponentProps<'input'>, 'className'> {
    actionText: string;
    onActionClicked?: PillProps['onClick'];
    actionType?: PillProps['type'];
    name: string;
    prefix?: string;
    suffix?: string;
    actionDisabled?: boolean;
    allowNegative?: boolean;
}

export function InputWithAction({
    actionText,
    onActionClicked,
    allowNegative,
    actionType = 'submit',
    type = 'number',
    disabled = false,
    actionDisabled = false,
    name,
    prefix,
    suffix,
    ...props
}: InputWithActionProps) {
    const [field, meta] = useField(name);
    const { isSubmitting } = useFormikContext();
    const isInputDisabled = isSubmitting || disabled;
    const shareStyle = cx(
        'transition flex flex-row items-center p-3 bg-white text-body font-semibold',
        'placeholder:text-gray-60 w-full pr-[calc(20%_+_24px)] rounded-md shadow-button',
        'border-solid border border-gray-45 text-steel-darker hover:border-steel focus:border-steel',
        'disabled:border-gray-40 disabled:text-gray-55'
    );
    return (
        <>
            <div className="flex flex-row flex-nowrap items-center relative">
                {type === 'number' ? (
                    <NumericFormat
                        valueIsNumericString
                        disabled={isInputDisabled}
                        prefix={prefix}
                        suffix={suffix}
                        {...field}
                        className={shareStyle}
                    />
                ) : (
                    <input
                        type={type}
                        disabled={isInputDisabled}
                        {...field}
                        {...props}
                        className={shareStyle}
                    />
                )}
                <div className="flex items-center justify-end absolute right-0 max-w-[20%] mx-3 overflow-hidden">
                    <Pill
                        text={actionText}
                        type={actionType}
                        disabled={actionDisabled || isInputDisabled}
                        loading={isSubmitting}
                        onClick={onActionClicked}
                    />
                </div>
            </div>
            {meta?.touched && meta?.error ? (
                <div className="mt-3">
                    <Alert>{meta?.error}</Alert>
                </div>
            ) : null}
        </>
    );
}
