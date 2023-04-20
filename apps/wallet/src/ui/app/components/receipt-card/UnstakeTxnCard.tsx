// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { Card } from '../../shared/transaction-summary/Card';
import { ValidatorLogo } from '_app/staking/validators/ValidatorLogo';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { Text } from '_src/ui/app/shared/text';

import type { SuiEvent } from '@mysten/sui.js';

type UnStakeTxnCardProps = {
    event: SuiEvent;
};

export function UnStakeTxnCard({ event }: UnStakeTxnCardProps) {
    const principalAmount = event.parsedJson?.principal_amount || 0;
    const rewardAmount = event.parsedJson?.reward_amount || 0;
    const validatorAddress = event.parsedJson?.validator_address;
    const totalAmount = Number(principalAmount) + Number(rewardAmount);
    const [formatPrinciple, symbol] = useFormatCoin(
        principalAmount,
        SUI_TYPE_ARG
    );
    const [formatRewards] = useFormatCoin(rewardAmount || 0, SUI_TYPE_ARG);

    return (
        <Card>
            <div className="flex flex-col divide-y divide-solid divide-gray-40 divide-x-0">
                {validatorAddress && (
                    <div className="mb-3.5 w-full">
                        <ValidatorLogo
                            validatorAddress={validatorAddress}
                            showAddress
                            iconSize="md"
                            size="body"
                        />
                    </div>
                )}
                {totalAmount && (
                    <TxnAmount
                        amount={totalAmount}
                        coinType={SUI_TYPE_ARG}
                        label="Total"
                    />
                )}

                <div className="flex justify-between w-full py-3.5">
                    <div className="flex gap-1 items-baseline text-steel">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Your SUI Stake
                        </Text>
                    </div>

                    <div className="flex gap-1 items-baseline text-steel">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {formatPrinciple} {symbol}
                        </Text>
                    </div>
                </div>

                <div className="flex justify-between w-full py-3.5">
                    <div className="flex gap-1 items-baseline text-steel">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Staking Rewards Earned
                        </Text>
                    </div>

                    <div className="flex gap-1 items-baseline text-steel">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {formatRewards} {symbol}
                        </Text>
                    </div>
                </div>
            </div>
        </Card>
    );
}
