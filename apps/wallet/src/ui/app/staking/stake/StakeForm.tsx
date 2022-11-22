// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { useFormatCoin } from '_hooks';
import {
    DEFAULT_GAS_BUDGET_FOR_STAKE,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';

import st from './StakeForm.module.scss';

export type StakeFromProps = {
    submitError: string | null;
    // TODO(ggao): remove this if needed
    coinBalance: string;
    coinType: string;
    onClearSubmitError: () => void;
};

function StakeForm({
    submitError,
    // TODO(ggao): remove this if needed
    coinBalance,
    coinType,
    onClearSubmitError,
}: StakeFromProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount },
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount]);

    const [formatted, symbol] = useFormatCoin(coinBalance, coinType);

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <div className={st.group}>
                <label className={st.label}>Amount:</label>
                <Field
                    component={NumberInput}
                    allowNegative={false}
                    name="amount"
                    placeholder={`Total ${symbol} to stake`}
                    className={st.input}
                    decimals
                />
                <div className={st.muted}>
                    Available balance: {formatted} {symbol}
                </div>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />
            </div>
            <div className={st.group}>
                * Total transaction fee estimate (gas cost):{' '}
                {DEFAULT_GAS_BUDGET_FOR_STAKE} {GAS_SYMBOL}
            </div>
            {submitError ? (
                <div className={st.group}>
                    <Alert>
                        <strong>Stake failed.</strong>{' '}
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
                    {isSubmitting ? <LoadingIndicator /> : 'Stake'}
                </button>
            </div>
        </Form>
    );
}

export default memo(StakeForm);
