// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';
import { useIntl } from 'react-intl';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import AddressInput from '_components/address-input';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { DEFAULT_GAS_BUDGET_FOR_TRANSFER } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinBalance: string;
    coinSymbol: string;
    coinType: string;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinBalance,
    coinSymbol,
    coinType,
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

    const totalAmount = parseFloat(amount) + DEFAULT_GAS_BUDGET_FOR_TRANSFER;

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <BottomMenuLayout>
                <Content>
                    <div className={cl(st.group, st.address)}>
                        <label className={st.label}>To:</label>
                        <Field
                            component={AddressInput}
                            name="to"
                            className={st.input}
                        />
                        <ErrorMessage
                            className={st.error}
                            name="to"
                            component="div"
                        />
                    </div>

                    {submitError ? (
                        <div className={st.error}>{submitError}</div>
                    ) : null}

                    <div className={st.responseCard}>
                        <div className={st.amount}>
                            {intl.formatNumber(BigInt(amount || 0))}{' '}
                            <span>{coinSymbol}</span>
                        </div>

                        <div className={cl(st.txFees, st.details)}>
                            <div className={st.txInfoLabel}>Gas Fee</div>
                            <div className={st.walletInfoValue}>
                                40 {coinSymbol}
                            </div>
                        </div>

                        <div className={st.txDate}>
                            <div className={st.txInfoLabel}>Total Amount</div>
                            <div className={st.walletInfoValue}>
                                {totalAmount} {coinSymbol}
                            </div>
                        </div>
                    </div>
                </Content>
                <Menu stuckClass={st.shadow}>
                    <div className={cl(st.group, st.cta)}>
                        <button
                            type="submit"
                            disabled={!isValid || isSubmitting}
                            className="btn"
                        >
                            {isSubmitting ? <LoadingIndicator /> : 'Send'}
                        </button>
                    </div>
                </Menu>
            </BottomMenuLayout>
        </Form>
    );
}

export default memo(StepTwo);
