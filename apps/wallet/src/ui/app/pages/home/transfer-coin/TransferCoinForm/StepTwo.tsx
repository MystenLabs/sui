// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import { useFormikContext } from 'formik';
import { useCallback } from 'react';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { useAppSelector, useFormatCoin } from '_hooks';
import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

export type TransferCoinFormProps = {
    coinType: string;
    gasCostEstimation: number | null;
};

export function StepTwo({
    coinType,
    gasCostEstimation,
}: TransferCoinFormProps) {
    const {
        values: { amount, to },
    } = useFormikContext<FormValues>();

    const accountAddress = useAppSelector(({ account }) => account.address);
    const [formattedGas] = useFormatCoin(gasCostEstimation, GAS_TYPE_ARG);

    return (
        <BottomMenuLayout>
            <Content>
                <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col px-2.5">
                    <TxnAmount
                        amount={amount}
                        label="Sending"
                        coinType={coinType}
                    />
                    <TxnAddress address={accountAddress || ''} label="From" />
                    <TxnAddress address={to} label="To" />
                    <div className="pt-3.5 mb-5 flex w-full gap-2 justify-between">
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
                </div>
            </Content>

            <Menu
                stuckClass="sendCoin-cta"
                className="w-full px-0 pb-0 mx-0 gap-2.5"
            >
                <Button
                    type="button"
                    mode="neutral"
                    className="w-full text-steel-darker"
                >
                    <ArrowLeft16 /> Back
                </Button>
                <Button type="submit" mode="primary" className="w-full">
                    Send Now <ArrowRight16 />
                </Button>
            </Menu>
        </BottomMenuLayout>
    );
}
