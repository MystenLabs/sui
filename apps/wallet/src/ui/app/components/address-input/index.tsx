// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, useField } from 'formik';
import { memo, useCallback, useMemo } from 'react';
import TextareaAutosize from 'react-textarea-autosize';

import { SUI_ADDRESS_VALIDATION } from './validation';
import { Text } from '_app/shared/text';
import Alert from '_src/ui/app/components/alert';

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
    form: { isSubmitting, dirty, setFieldValue },
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

    const [, { touched, error: addressError }] = useField(name);

    return (
        <>
            <div
                className={cl(
                    st.group,
                    dirty && addressError ? st.invalidAddr : ''
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
                        className="w-full py-3.5 px-3 flex items-center rounded-2lg text-gray-90 text-bodySmall leading-130 font-medium font-mono bg-white placeholder:text-steel-dark placeholder:font-normal placeholder:font-mono border border-solid border-gray-45 box-border focus:border-steel transition-all"
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

            {!addressError && touched && (
                <div className="mt-2 w-full">
                    <Alert mode="success">
                        <Text variant="bodySmall" weight="medium">
                            Valid address
                        </Text>
                    </Alert>
                </div>
            )}
        </>
    );
}

export default memo(AddressInput);
