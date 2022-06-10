// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';
import { useIntl } from 'react-intl';

import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import {
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { FormValues } from './';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinBalance: string;
    coinSymbol: string;
    onClearSubmitError: () => void;
};

function TransferCoinForm({
    submitError,
    coinBalance,
    coinSymbol,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount, to },
    } = useFormikContext<FormValues>();
    const intl = useIntl();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);
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
                    placeholder={`Total ${coinSymbol.toLocaleUpperCase()} to send`}
                    className={st.input}
                />
                <div className={st.muted}>
                    Available balance:{' '}
                    {intl.formatNumber(
                        BigInt(coinBalance),
                        balanceFormatOptions
                    )}{' '}
                    {coinSymbol}
                </div>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />
            </div>
            <div className={st.group}>
                * Total transaction fee estimate (gas cost):{' '}
                {DEFAULT_GAS_BUDGET_FOR_TRANSFER} {GAS_SYMBOL}
            </div>
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

export default memo(TransferCoinForm);
