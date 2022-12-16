// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useCallback, useMemo } from 'react';

import { formatBalance } from '../../hooks/useFormatCoin';
import { Content } from '_app/shared/bottom-menu-layout';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import NumberInput from '_components/number-input';
import { parseAmount } from '_helpers';
import { useFormatCoin, useCoinDecimals, useAppSelector } from '_hooks';
import { accountCoinsSelector } from '_redux/slices/account';
import { GAS_SYMBOL, Coin } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from './StakingCard';

import st from './StakeForm.module.scss';

export type StakeFromProps = {
    submitError: string | null;
    // TODO(ggao): remove this if needed
    coinBalance: string;
    coinType: string;
    unstake: boolean;
    onClearSubmitError: () => void;
};

function StakeForm({
    submitError,
    // TODO(ggao): remove this if needed
    coinBalance,
    coinType,
    unstake,
    onClearSubmitError,
}: StakeFromProps) {
    const {
        setFieldValue,
        values: { amount },
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount]);

    const [formatted, symbol] = useFormatCoin(coinBalance, coinType);

    const allCoins = useAppSelector(accountCoinsSelector);
    const allCoinsOfTransferType = useMemo(
        () => allCoins.filter((aCoin) => aCoin.type === coinType),
        [allCoins, coinType]
    );
    const [coinDecimals] = useCoinDecimals(coinType);
    const gasEstimationUnformatted = useMemo(() => {
        return Coin.computeGasBudgetForPay(
            allCoinsOfTransferType,
            parseAmount(amount, coinDecimals)
        );
    }, [allCoinsOfTransferType, amount, coinDecimals]);

    const [gasBudgetEstimation] = useFormatCoin(
        gasEstimationUnformatted,
        coinType
    );

    const maxToken = formatBalance(coinBalance, coinDecimals);
    const setMaxToken = useCallback(() => {
        setFieldValue('amount', maxToken);
    }, [maxToken, setFieldValue]);

    return (
        <Form
            className="flex flex-1 flex-col flex-nowrap"
            autoComplete="off"
            noValidate={true}
        >
            <Content>
                <Card
                    variant="blue"
                    header={
                        <div className="p-2.5 w-full flex bg-white">
                            <Field
                                component={NumberInput}
                                allowNegative={false}
                                name="amount"
                                className="w-full border-none text-hero-dark text-heading4 font-semibold  placeholder:text-gray-70 placeholder:font-medium"
                                decimals
                            />
                            <div
                                role="button"
                                className="border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker  hover:text-steel-darker text-bodySmall font-medium"
                                onClick={setMaxToken}
                            >
                                Max
                            </div>
                        </div>
                    }
                    footer={
                        <div className="py-px flex justify-between w-full">
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                Gas Fee
                            </Text>
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                {gasBudgetEstimation} {GAS_SYMBOL}
                            </Text>
                        </div>
                    }
                >
                    {!unstake && (
                        <div className="flex justify-between w-full mb-3.5">
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                Available balance:
                            </Text>
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                {formatted} {symbol}
                            </Text>
                        </div>
                    )}
                </Card>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />

                {submitError ? (
                    <div className="mt-2 flex flex-col flex-nowrap">
                        <Alert mode="warning">
                            <strong>
                                {unstake ? 'UnStake failed' : 'Stake failed'}.
                            </strong>
                            <small>{submitError}</small>
                        </Alert>
                    </div>
                ) : null}
            </Content>
        </Form>
    );
}

export default memo(StakeForm);
