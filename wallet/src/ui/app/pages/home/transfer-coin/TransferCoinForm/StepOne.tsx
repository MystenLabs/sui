// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';
import { useIntl } from 'react-intl';

import Button from '_app/shared/button';
import Alert from '_components/alert';
import ActiveCoinCard from '_components/coin-selection';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import {
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { FormValuesStepOne } from '../';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinBalance: string;
    coinSymbol: string;
    coinType: string;
    onClearSubmitError: () => void;
};

function StepOne({
    submitError,
    coinBalance,
    coinSymbol,
    coinType,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount },
    } = useFormikContext<FormValuesStepOne>();
    const intl = useIntl();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount]);
    return (
        <Form
            className={cl(st.container, st.amount)}
            autoComplete="off"
            noValidate={true}
        >
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
            <ActiveCoinCard activeCoinType={coinType} />
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
                <Button
                    type="submit"
                    disabled={!isValid || isSubmitting}
                    mode="primary"
                    className={st.btn}
                >
                    Continue
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className={cl(st.arrowLeft)}
                    />
                </Button>
            </div>
        </Form>
    );
}

export default memo(StepOne);
