// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, type FormikErrors } from 'formik';
import { memo, useCallback, useMemo } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { SUI_ADDRESS_VALIDATION } from './validation';
import Icon, { SuiIcons } from '_components/icon';

import type { SuiAddress } from '@mysten/sui.js';
import type { FieldProps } from 'formik';
import type { ChangeEventHandler } from 'react';

import st from './AddressInput.module.scss';

export interface AddressInputProps<Values>
    extends FieldProps<SuiAddress, Values> {
    disabled?: boolean;
    placeholder?: string;
    className?: string;
}

// TODO: (Jibz) use Tailwind and match latest designs
function AddressInput<FormValues>({
    disabled: forcedDisabled,
    placeholder = '0x...',
    className,
    form: { isSubmitting, dirty, setFieldValue, isValid, errors },
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

    const { to: addressError } = errors as FormikErrors<{ to: string }>;

    return (
        <>
            <div
                className={cl(
                    st.group,
                    dirty && formattedValue !== '' && addressError
                        ? st.invalidAddr
                        : ''
                )}
            >
                <div className={st.textarea}>
                    <TextareaAutosize
                        maxRows={2}
                        minRows={1}
                        disabled={disabled}
                        placeholder={placeholder}
                        value={formattedValue}
                        onChange={handleOnChange}
                        onBlur={onBlur}
                        className={className}
                        name={name}
                    />
                </div>
                <div
                    onClick={clearAddress}
                    className={cl(
                        st.inputGroupAppend,
                        dirty && formattedValue !== ''
                            ? st.changeAddrIcon + ' sui-icons-close'
                            : st.qrCode
                    )}
                ></div>
            </div>

            <ErrorMessage className={st.error} name="to" component="div" />

            {!addressError && formattedValue !== '' && dirty && (
                <div className={st.validAddress}>
                    <Icon icon={SuiIcons.Checkmark} className={st.checkmark} />
                    Valid address
                </div>
            )}
        </>
    );
}

export default memo(AddressInput);
