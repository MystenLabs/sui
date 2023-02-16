// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { cx } from 'class-variance-authority';
import { useField, useFormikContext } from 'formik';

import Alert from '../components/alert';
import { Pill, type PillProps } from './Pill';
import NumberInput from '_components/number-input';

import type { ComponentProps } from 'react';

export interface InputWithActionProps
    extends Omit<ComponentProps<'input'>, 'className'> {
    actionText: string;
    onActionClicked?: PillProps['onClick'];
    actionType?: PillProps['type'];
    name: string;
    prefix?: string;
    suffix?: string;
    actionDisabled?: boolean | 'auto';
    allowNegative?: boolean;
    allowDecimals?: boolean;
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
    const form = useFormikContext();
    const { isSubmitting } = form;
    const isInputDisabled = isSubmitting || disabled;
    const isActionDisabled =
        actionDisabled === 'auto'
            ? isInputDisabled ||
              meta?.initialValue === meta?.value ||
              !!meta?.error
            : actionDisabled;
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
                    <NumberInput
                        className={shareStyle}
                        allowNegative
                        {...props}
                        form={form}
                        field={field}
                        meta={meta}
                        disabled={isInputDisabled}
                        decimals={true}
                        prefix={prefix}
                        suffix={suffix}
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
                        disabled={isActionDisabled}
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
