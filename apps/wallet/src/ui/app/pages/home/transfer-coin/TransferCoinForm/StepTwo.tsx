// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormikContext } from 'formik';
import { useRef, useMemo, useEffect } from 'react';

import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { TxnAddress } from '_components/receipt-card/TxnAddress';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { parseAmount } from '_helpers';
import { useAppSelector, useFormatCoin, useCoinDecimals } from '_hooks';
import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

export type TransferCoinFormProps = {
    coinType: string;
    gasCostEstimation: number | null;
    onClearSubmitError: () => void;
};

export function StepTwo({
    coinType,
    gasCostEstimation,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        values: { amount, to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);

    const accountAddress = useAppSelector(({ account }) => account.address);
    const [decimals] = useCoinDecimals(coinType);
    const amountWithoutDecimals = useMemo(
        () => parseAmount(amount, decimals),
        [amount, decimals]
    );

    const [formattedGas] = useFormatCoin(gasCostEstimation, GAS_TYPE_ARG);

    return (
        <div className="divide-y divide-solid divide-steel/20 divide-x-0 flex flex-col px-2.5">
            <TxnAmount
                amount={amountWithoutDecimals.toString()}
                label="Sending"
                coinType={coinType}
            />
            <TxnAddress address={accountAddress || ''} label="From" />
            <TxnAddress address={to} label="To" />
            <div className="pt-3.5 mb-5 flex w-full gap-2 justify-between">
                <div className="flex gap-2 ">
                    <Text variant="body" color="gray-80" weight="medium">
                        Estimated Gas Fees
                    </Text>
                    <div className="text-gray-60">
                        <IconTooltip tip="Estimated Gas Fees" placement="top" />
                    </div>
                </div>
                <Text variant="body" color="gray-90" weight="medium">
                    {formattedGas} {GAS_SYMBOL}
                </Text>
            </div>
        </div>
    );
}
