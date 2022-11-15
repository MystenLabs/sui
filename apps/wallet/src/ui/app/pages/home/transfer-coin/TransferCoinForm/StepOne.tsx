// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import cl from 'classnames';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import ActiveCoinsCard from '_components/active-coins-card';
import Icon, { SuiIcons } from '_components/icon';
import NumberInput from '_components/number-input';
import { useCoinDecimals } from '_hooks';

import type { FormValues } from '../';

import st from './TransferCoinForm.module.scss';

function parseAmount(amount: string, coinDecimals: number) {
    try {
        return BigInt(
            new BigNumber(amount)
                .shiftedBy(coinDecimals)
                .integerValue()
                .toString()
        );
    } catch (e) {
        return BigInt(0);
    }
}

export type TransferCoinFormProps = {
    coinSymbol: string;
    coinType: string;
    onClearSubmitError: () => void;
    onAmountChanged: (amount: bigint) => void;
};

function StepOne({
    coinSymbol,
    coinType,
    onClearSubmitError,
    onAmountChanged,
}: TransferCoinFormProps) {
    const {
        isValid,
        validateForm,
        values: { amount },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount]);
    const [coinDecimals, { isLoading: isCoinDecimalsLoading }] =
        useCoinDecimals(coinType);
    useEffect(() => {
        if (!isCoinDecimalsLoading) {
            const parsedAmount = parseAmount(amount, coinDecimals);
            onAmountChanged(parsedAmount);
            // seems changing the validationSchema doesn't rerun the validation for the form
            // trigger re-validation here when the amount to send is changed
            // (changing the amount will probably change the gasBudget and in the end the validationSchema)
            validateForm();
        }
    }, [
        amount,
        onAmountChanged,
        coinDecimals,
        isCoinDecimalsLoading,
        validateForm,
    ]);
    return (
        <Form
            className={cl(st.container, st.amount)}
            autoComplete="off"
            noValidate={true}
        >
            <Content>
                <div className={st.group}>
                    <label className={st.label}>Amount:</label>
                    <Field
                        component={NumberInput}
                        allowNegative={false}
                        name="amount"
                        placeholder={`Total ${coinSymbol.toLocaleUpperCase()} to send`}
                        className={st.input}
                        decimals
                    />

                    <ErrorMessage
                        className={st.error}
                        name="amount"
                        component="div"
                    />
                </div>
                <div className={st.activeCoinCard}>
                    <ActiveCoinsCard activeCoinType={coinType} />
                </div>
            </Content>
            <Menu stuckClass={st.shadow}>
                <div className={cl(st.group, st.cta)}>
                    <Button
                        type="submit"
                        disabled={!isValid}
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
            </Menu>
        </Form>
    );
}

export default memo(StepOne);
