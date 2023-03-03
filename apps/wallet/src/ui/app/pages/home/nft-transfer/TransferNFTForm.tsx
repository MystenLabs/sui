// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Form, Field, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { AddressInput } from '_components/address-input';
import Alert from '_components/alert';
import { useAppSelector, useGetCoinBalance } from '_hooks';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { FormValues } from '.';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBudget: number;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    submitError,
    gasBudget,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { to },
        submitForm,
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to]);

    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data: coinBalance, isLoading: loadingBalances } = useGetCoinBalance(
        SUI_TYPE_ARG,
        accountAddress
    );

    const maxGasCoinBalance = coinBalance?.totalBalance || BigInt(0);
    const { gasBudget: gasBudgetInMist } = useGasBudgetInMist(gasBudget);
    const isInsufficientGas = maxGasCoinBalance < BigInt(gasBudgetInMist || 0);
    return (
        <Form autoComplete="off" noValidate={true} className="h-full">
            <BottomMenuLayout className="h-full">
                <Content>
                    <div className="flex gap-2.5 flex-col">
                        <div className="px-2.5 tracking-wider">
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
                                placeholder="Enter Address"
                            />
                        </div>

                        {submitError ? (
                            <div className="mt-3 w-full">
                                <Alert>{submitError}</Alert>
                            </div>
                        ) : null}
                    </div>
                    {isInsufficientGas ? (
                        <div className="mt-2.5">
                            <Alert>
                                Insufficient balance, no individual coin found
                                with enough balance to cover for the transfer
                                cost
                            </Alert>
                        </div>
                    ) : null}
                    {submitError ? (
                        <div className="mt-2.5">
                            <Alert>{submitError}</Alert>
                        </div>
                    ) : null}
                </Content>
                <Menu
                    stuckClass="sendCoin-cta"
                    className="w-full px-0 pb-0 mx-0 gap-2.5"
                >
                    <Button
                        type="submit"
                        variant="primary"
                        loading={isSubmitting}
                        onClick={submitForm}
                        disabled={
                            !isValid ||
                            isSubmitting ||
                            isInsufficientGas ||
                            !gasBudget ||
                            !gasBudgetInMist ||
                            loadingBalances
                        }
                        size="tall"
                        text="Send NFT Now"
                        after={<ArrowRight16 />}
                    />
                </Menu>
            </BottomMenuLayout>
        </Form>
    );
}

export default memo(TransferNFTForm);
