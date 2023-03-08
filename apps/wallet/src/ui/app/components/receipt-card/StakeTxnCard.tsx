// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { useMemo } from 'react';

import { calculateAPY } from '_app/staking/calculateAPY';
import { ValidatorLogo } from '_app/staking/validators/ValidatorLogo';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';
import { useSystemState } from '_src/ui/app/staking/useSystemState';

import type {
    TransactionEffects,
    MoveEvent,
    TransactionEvents,
} from '@mysten/sui.js';

type StakeTxnCardProps = {
    txnEffects: TransactionEffects;
    events: TransactionEvents;
};

const REQUEST_DELEGATION_EVENT = '0x2::validator_set::DelegationRequestEvent';

// TODO: moveEvents is will be changing
// For Staked Transaction use moveEvent Field to get the validator address, delegation amount, epoch
export function StakeTxnCard({ txnEffects, events }: StakeTxnCardProps) {
    const stakingData = useMemo(() => {
        if (!events) return null;

        const event = events.find(
            (event) =>
                'moveEvent' in event &&
                event.moveEvent.type === REQUEST_DELEGATION_EVENT
        );
        if (!event) return null;
        const { moveEvent } = event as { moveEvent: MoveEvent };
        return moveEvent;
    }, [events]);

    const { data: system } = useSystemState();

    const validatorData = useMemo(() => {
        if (!system || !stakingData || !stakingData.fields.validator_address)
            return null;
        return system.active_validators.find(
            (av) => av.sui_address === stakingData.fields.validator_address
        );
    }, [stakingData, system]);

    const apy = useMemo(() => {
        if (!validatorData || !system) return 0;
        return calculateAPY(validatorData, +system.epoch);
    }, [validatorData, system]);

    const rewardEpoch = useMemo(() => {
        if (!system || !stakingData?.fields.epoch) return 0;
        // show reward epoch only after 2 epochs
        const rewardStarts = +stakingData.fields.epoch + 2;
        return +system.epoch > rewardStarts ? rewardStarts : 0;
    }, [stakingData, system]);

    return (
        <>
            {stakingData?.fields.validator_address && (
                <div className="mb-3.5 w-full">
                    <ValidatorLogo
                        validatorAddress={stakingData.fields.validator_address}
                        showAddress
                        iconSize="md"
                        size="body"
                    />
                </div>
            )}

            <div className="flex justify-between w-full py-3.5">
                <div className="flex gap-1 items-baseline text-steel">
                    <Text variant="body" weight="medium" color="steel-darker">
                        APY
                    </Text>
                    <IconTooltip tip="This is the Annualized Percentage Yield of the a specific validator’s past operations. Note there is no guarantee this APY will be true in the future." />
                </div>
                <Text variant="body" weight="medium" color="steel-darker">
                    {apy && apy > 0 ? apy + ' %' : '--'}
                </Text>
            </div>

            {stakingData?.fields.amount && (
                <TxnAmount
                    amount={stakingData.fields.amount}
                    coinType={SUI_TYPE_ARG}
                    label="Stake"
                />
            )}
            {rewardEpoch > 0 && (
                <div className="flex justify-between w-full py-3.5">
                    <div className="flex gap-1 items-baseline text-steel">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Staking Rewards Start
                        </Text>
                        <IconTooltip tip="This is the Annualized Percentage Yield of the a specific validator’s past operations. Note there is no guarantee this APY will be true in the future." />
                    </div>

                    <Text variant="body" weight="medium" color="steel-darker">
                        Epoch #{rewardEpoch}
                    </Text>
                </div>
            )}
        </>
    );
}
