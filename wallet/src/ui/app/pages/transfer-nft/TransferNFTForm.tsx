// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';
import { useIntl } from 'react-intl';

import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { balanceFormatOptions } from '_shared/formatting';

import type { FormValues } from '.';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBalance: string;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    submitError,
    gasBalance,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { to, amount },
    } = useFormikContext<FormValues>();
    const intl = useIntl();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    const transferCost = 10000;
    useEffect(() => {
        onClearRef.current();
    }, [to, amount]);
    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <div className={st.group}>
                <label className={st.label}>To:</label>
                <Field
                    component={AddressInput}
                    name="to"
                    className={st.input}
                />
                <div className={st.muted}>The recipient&apos;s address</div>
                <ErrorMessage className={st.error} name="to" component="div" />
            </div>

            <div className={st.group}>
                <label className={st.label}>Amount:</label>
                <Field
                    component={NumberInput}
                    allowNegative={false}
                    name="amount"
                    value={transferCost}
                    className={st.input}
                    disabled={true}
                />
                <div className={st.muted}>
                    Available balance:{' '}
                    {intl.formatNumber(
                        BigInt(gasBalance),
                        balanceFormatOptions
                    )}{' '}
                </div>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />
            </div>

            {BigInt(gasBalance) < transferCost && (
                <div className={st.error}>
                    * Insufficient balance to cover transfer cost
                </div>
            )}
            {submitError ? (
                <div className={st.group}>
                    <Alert>
                        <strong>Transfer failed.</strong>{' '}
                        <small>{submitError}</small>
                    </Alert>
                </div>
            ) : null}
            <div className={st.group}>
                <button
                    type="submit"
                    disabled={!isValid || isSubmitting}
                    className="btn"
                >
                    {isSubmitting ? <LoadingIndicator /> : 'Send'}
                </button>
            </div>
        </Form>
    );
}

export default memo(TransferNFTForm);
