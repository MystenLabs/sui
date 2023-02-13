// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import cl from 'classnames';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { parseAmount } from '_helpers';
import { useCoinDecimals, useFormatCoin } from '_hooks';
import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinSymbol: string;
    coinType: string;
    gasBudgetEstimation: number | null;
    gasCostEstimation: number | null;
    gasEstimationLoading?: boolean;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinSymbol,
    coinType,
    gasBudgetEstimation,
    gasCostEstimation,
    gasEstimationLoading,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount, to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);
    const [decimals] = useCoinDecimals(coinType);
    const amountWithoutDecimals = useMemo(
        () => parseAmount(amount, decimals),
        [amount, decimals]
    );
    const totalSuiAmount = new BigNumber(gasCostEstimation || 0)
        .plus(GAS_SYMBOL === coinSymbol ? amountWithoutDecimals.toString() : 0)
        .toString();
    const [formattedBalance] = useFormatCoin(amountWithoutDecimals, coinType);
    const [formattedTotalSui] = useFormatCoin(totalSuiAmount, GAS_TYPE_ARG);
    const [formattedGas] = useFormatCoin(gasCostEstimation, GAS_TYPE_ARG);

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <Content>
                <div className="w-full flex gap-2.5 flex-col">
                    <div className="px-2 tracking-wider">
                        <Text
                            variant="caption"
                            color="steel-dark"
                            weight="semibold"
                        >
                            Select SUI Amount to Send
                        </Text>
                    </div>
                    <div className="w-full flex relative items-center">
                        <Field
                            component={NumberInput}
                            allowNegative={false}
                            name="amount"
                            placeholder="0.00"
                            suffix={coinSymbol}
                            className="w-full h-11 py-3 px-3 pr-14 flex items-center rounded-2lg text-steel-dark text-body font-semibold bg-white placeholder:text-steel placeholder:font-semibold border border-solid border-gray-45 box-border focus:border-steel transition-all"
                            decimals
                        />
                        <button
                            className="absolute right-3 bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
                            type="button"
                        >
                            Max
                        </button>
                    </div>

                    <div className="px-2 mt-1 mb-5 flex w-full gap-2 justify-between">
                        <div className="flex gap-2 ">
                            <Text
                                variant="body"
                                color="gray-80"
                                weight="medium"
                            >
                                Estimated Gas Fees
                            </Text>
                            <div className="text-gray-60">
                                <IconTooltip
                                    tip="Estimated Gas Fees"
                                    placement="top"
                                />
                            </div>
                        </div>
                        <Text variant="body" color="gray-90" weight="medium">
                            {formattedGas} {GAS_SYMBOL}
                        </Text>
                    </div>

                    <div className="px-2 tracking-wider">
                        <Text
                            variant="caption"
                            color="steel-dark"
                            weight="semibold"
                        >
                            Enter Recipient Address
                        </Text>
                    </div>
                    <div className="w-full flex relative items-center flex-col">
                        <Field
                            component={AddressInput}
                            allowNegative={false}
                            name="to"
                            className="w-full py-3.5 px-3  flex items-center rounded-2lg text-steel-dark text-body font-semibold bg-white placeholder:text-steel placeholder:font-semibold border border-solid border-gray-45 box-border focus:border-steel transition-all"
                        />
                    </div>

                    {submitError ? (
                        <div className="mt-3 w-full">
                            <Alert>{submitError}</Alert>
                        </div>
                    ) : null}
                </div>
            </Content>
            <Menu stuckClass={st.shadow}>
                <div className={cl(st.group, st.cta)}>
                    <Button
                        type="submit"
                        disabled={
                            !isValid ||
                            to === '' ||
                            isSubmitting ||
                            !gasBudgetEstimation
                        }
                        mode="primary"
                        className={st.btn}
                    >
                        {isSubmitting ||
                        (gasEstimationLoading && !gasBudgetEstimation) ? (
                            <LoadingIndicator />
                        ) : (
                            'Send Coins Now'
                        )}
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

export default memo(StepTwo);
