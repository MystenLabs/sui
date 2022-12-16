// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useCallback } from 'react';

import { formatBalance } from '../../hooks/useFormatCoin';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { useFormatCoin, useCoinDecimals } from '_hooks';
import {
    DEFAULT_GAS_BUDGET_FOR_STAKE,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from './StakingCard';

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
        setFieldValue,
        values: { amount },
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount]);

    const [formatted, symbol] = useFormatCoin(coinBalance, coinType);
    const [coinDecimals] = useCoinDecimals(coinType);
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
                        <div className="p-3 w-full flex bg-white">
                            <Field
                                component={NumberInput}
                                allowNegative={false}
                                name="amount"
                                placeholder={`Total ${symbol} to stake`}
                                className="w-full border-none text-hero-dark text-heading4 font-semibold  placeholder:text-gray-70 placeholder:font-medium"
                                decimals
                            />
                            <div
                                role="button"
                                className="border border-solid border-gray-60 rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:bg-gray-60 hover:text-white  text-bodySmall font-medium"
                                onClick={setMaxToken}
                            >
                                Max
                            </div>
                        </div>
                    }
                    footer={
                        <div className="flex justify-between w-full">
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
                                {DEFAULT_GAS_BUDGET_FOR_STAKE} {GAS_SYMBOL}
                            </Text>
                        </div>
                    }
                >
                    <div className="flex justify-between w-full">
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
                </Card>
                <ErrorMessage
                    className={st.error}
                    name="amount"
                    component="div"
                />

                {submitError ? (
                    <div className="mt-2 flex flex-col flex-nowrap">
                        <Alert mode="warning">
                            <strong>Stake failed.</strong>{' '}
                            <small>{submitError}</small>
                        </Alert>
                    </div>
                ) : null}
            </Content>
            <Menu stuckClass="staked-cta" className="w-full px-0 pb-0 mx-0">
                <Button
                    size="large"
                    mode="neutral"
                    href="new"
                    title="Currently disabled"
                    className="!text-steel-darker w-1/2"
                >
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className="text-body text-gray-65 font-normal"
                    />
                    Back
                </Button>
                <Button
                    size="large"
                    mode="primary"
                    type="submit"
                    title="Currently disabled"
                    className=" w-1/2"
                    disabled={!isValid || isSubmitting}
                >
                    {isSubmitting ? <LoadingIndicator /> : 'Stake Now'}
                </Button>
            </Menu>
        </Form>
    );
}

export default memo(StakeForm);
