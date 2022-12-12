// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useCallback, useEffect, useRef, memo } from 'react';

import { formatBalance } from '../../hooks/useFormatCoin';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Card, { CardFooter, CardHeader } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { useCoinDecimals } from '_hooks';

import type { FormValues } from './StakingCard';

import st from './StakeForm.module.scss';

export type StakeFromProps = {
    submitError: string | null;
    coinBalance: string;
    coinType: string;
    onClearSubmitError: () => void;
};

function StakeForm({
    submitError,
    coinBalance,
    coinType,
    onClearSubmitError,
}: StakeFromProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount },
        setFieldValue,
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    const [coinDecimals] = useCoinDecimals(coinType);
    const maxToken = formatBalance(coinBalance, coinDecimals);

    useEffect(() => {
        onClearRef.current();
    }, [amount]);

    const setMaxToken = useCallback(() => {
        setFieldValue('amount', maxToken);
    }, [maxToken, setFieldValue]);

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <Content>
                <div className={st.group}>
                    <Card>
                        <CardHeader background="transparent">
                            <div className="p-3 w-full flex">
                                <Field
                                    component={NumberInput}
                                    allowNegative={false}
                                    name="amount"
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
                        </CardHeader>

                        <CardFooter className="bg-sui/10">
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
                                0 SUI
                            </Text>
                        </CardFooter>
                    </Card>
                    <ErrorMessage
                        className={st.error}
                        name="amount"
                        component="div"
                    />
                </div>
                {submitError ? (
                    <div className={st.error}>{submitError}</div>
                ) : null}
            </Content>
            <Menu stuckClass="flex pb-0">
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
