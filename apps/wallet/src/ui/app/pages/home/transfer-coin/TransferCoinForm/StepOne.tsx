// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useCallback } from 'react';

import { Button } from '_app/shared/ButtonUI';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import NumberInput from '_components/number-input';
import { parseAmount } from '_helpers';
import { useCoinDecimals, useFormatCoin } from '_hooks';
import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinSymbol: string;
    coinType: string;
    gasBudgetEstimation: number | null;
    gasCostEstimation: number | null;
    gasEstimationLoading?: boolean;
    onClearSubmitError: () => void;
    onAmountChanged: (amount: bigint) => void;
    balance: bigint | null;
};

function StepOne({
    submitError,
    coinType,
    onClearSubmitError,
    onAmountChanged,
    gasBudgetEstimation,
    gasCostEstimation,
    gasEstimationLoading,
    balance,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        validateForm,
        values: { amount, to },
        errors,
        touched,
        setFieldValue,
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);
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
    const [formattedGas] = useFormatCoin(gasCostEstimation, GAS_TYPE_ARG);
    const [maxToken, symbol, queryResult] = useFormatCoin(balance, coinType);

    const setMaxToken = useCallback(() => {
        if (!maxToken) return;
        setFieldValue('amount', maxToken);
    }, [maxToken, setFieldValue]);

    return (
        <Form autoComplete="off" noValidate>
            <Content>
                <div className="w-full flex gap-2.5 flex-col">
                    <div className="px-2">
                        <Text
                            variant="captionSmall"
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
                            suffix={symbol}
                            className="w-full h-11 py-3 px-3 pr-14 flex items-center rounded-2lg text-steel-dark text-body font-semibold bg-white placeholder:text-steel placeholder:font-semibold border border-solid border-gray-45 box-border focus:border-steel transition-all"
                            decimals
                        />
                        <button
                            className="absolute right-3 bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
                            type="button"
                            onClick={setMaxToken}
                            disabled={queryResult.isLoading || !balance}
                        >
                            Max
                        </button>
                    </div>
                    {errors.amount && touched.amount ? (
                        <div className="mt-1">
                            <Alert>{errors.amount}</Alert>
                        </div>
                    ) : null}
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

                    <div className="px-2">
                        <Text
                            variant="captionSmall"
                            color="steel-dark"
                            weight="semibold"
                        >
                            Enter Recipient Address
                        </Text>
                    </div>
                    <div className="w-full flex relative items-center  flex-col">
                        <Field
                            component={AddressInput}
                            allowNegative={false}
                            name="to"
                            placeholder="Enter Address"
                            className="w-full py-3.5 px-3  flex items-center rounded-2lg text-gray-90 text-body font-medium font-mono bg-white placeholder:text-steel-dark placeholder:font-normal placeholder:font-mono border border-solid border-gray-45 box-border focus:border-steel transition-all"
                        />
                    </div>

                    {submitError ? (
                        <div className="mt-3">
                            <Alert>{submitError}</Alert>
                        </div>
                    ) : null}
                </div>
            </Content>
            <Menu stuckClass="sendCoin-cta" className="w-full px-0 pb-0 mx-0">
                <Button
                    type="submit"
                    disabled={
                        !isValid ||
                        to === '' ||
                        isSubmitting ||
                        !gasBudgetEstimation
                    }
                    variant="primary"
                    text="Review"
                    after={<ArrowRight16 />}
                />
            </Menu>
        </Form>
    );
}

export default memo(StepOne);
