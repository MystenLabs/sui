// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';
import { useFormatCoin } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

type GasCardProps = {
    totalGasUsed: number;
    computationCost: number;
    storageCost: number;
    storageRebate: number;
    totalAmount?: number | null;
};

// TODO add expandable gas properties
export function GasCard({
    totalGasUsed,
    computationCost,
    storageCost,
    storageRebate,
    totalAmount,
}: GasCardProps) {
    const [formattedTotalGas, symbol] = useFormatCoin(
        totalGasUsed,
        GAS_TYPE_ARG
    );
    const total = Math.abs(totalAmount || 0) + totalGasUsed;

    const [totalCost] = useFormatCoin(total, GAS_TYPE_ARG);

    return (
        <div className="flex flex-col gap-3.5 items-center w-full justify-between">
            <div className="flex items-center w-full justify-between">
                <div className="flex gap-0.5 items-center leading-none">
                    <Text variant="body" weight="medium" color="steel-darker">
                        Gas Fees
                    </Text>

                    <Icon
                        className="text-caption steel-darker font-thin"
                        icon={SuiIcons.ChevronDown}
                    />
                </div>
                <div className="flex gap-0.5">
                    <Text variant="body" weight="medium" color="steel-darker">
                        {formattedTotalGas}
                    </Text>
                    <Text variant="body" weight="medium" color="steel-darker">
                        {symbol}
                    </Text>
                </div>
            </div>

            {!!totalAmount && (
                <div className="flex flex-row items-center w-full justify-between">
                    <Text variant="body" weight="medium" color="steel-darker">
                        Total Amount
                    </Text>

                    <div className="flex gap-0.5">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {totalCost}
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {symbol}
                        </Text>
                    </div>
                </div>
            )}
        </div>
    );
}
