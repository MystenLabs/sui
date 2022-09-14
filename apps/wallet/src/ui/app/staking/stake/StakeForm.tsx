// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin } from '@mysten/sui.js';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { useCoinFormat } from '_app/shared/coin-balance/coin-format';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import {
    DEFAULT_GAS_BUDGET_FOR_STAKE,
    GAS_TYPE_ARG,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';

import st from './StakeForm.module.scss';

export type StakeFromProps = {
    submitError: string | null;
    // TODO(ggao): remove this if needed
    coinBalance: bigint;
    coinTypeArg: string;
    onClearSubmitError: () => void;
};

function StakeForm({
    submitError,
    // TODO(ggao): remove this if needed
    coinBalance,
    coinTypeArg,
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
    const balanceFormatted = useCoinFormat(
        coinBalance,
        coinTypeArg,
        'accurate'
    ).displayFull;
    const coinSymbol = useMemo(
        () => Coin.getCoinSymbol(coinTypeArg),
        [coinTypeArg]
    );
    const gasCostFormatted = useCoinFormat(
        BigInt(DEFAULT_GAS_BUDGET_FOR_STAKE),
        GAS_TYPE_ARG,
        'accurate'
    ).displayFull;
    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <div className={st.group}>
                <label className={st.label}>Amount:</label>
                <Field
                    component={NumberInput}
                    allowNegative={false}
                    name="amount"
                    placeholder={`Total ${coinSymbol.toLocaleUpperCase()} to stake`}
                    className={st.input}
                    decimals={true}
                />
                <div className={st.muted}>
                    Available balance: {balanceFormatted}
                </div>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />
            </div>
            <div className={st.group}>
                * Total transaction fee estimate (gas cost): {gasCostFormatted}
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
