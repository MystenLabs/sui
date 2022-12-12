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
};

// TODO add expandable gas properties
export function GasCard({
    totalGasUsed,
    computationCost,
    storageCost,
    storageRebate,
}: GasCardProps) {
    const [formattedTotalGas, symbol] = useFormatCoin(
        totalGasUsed,
        GAS_TYPE_ARG
    );

    return (
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
    );
}
