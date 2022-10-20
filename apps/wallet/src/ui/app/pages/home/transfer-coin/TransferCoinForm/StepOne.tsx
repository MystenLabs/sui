// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import ActiveCoinsCard from '_components/active-coins-card';
import Icon, { SuiIcons } from '_components/icon';
import NumberInput from '_components/number-input';

import type { FormValues } from '../';

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
        isValid,
        values: { amount },
    } = useFormikContext<FormValues>();

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
