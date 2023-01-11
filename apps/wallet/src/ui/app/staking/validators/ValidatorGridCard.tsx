// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import { ActiveDelegation } from '../home/ActiveDelegation';
import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { ValidatorLogo } from '../validator-detail/ValidatorLogo';
import { Text } from '_src/ui/app/shared/text';
import { DelegationCard, DelegationState } from './../home/DelegationCard';
import { useGetObject } from '_hooks';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

import type { SuiAddress } from '@mysten/sui.js';

export function ValidatorGridCard({
    validatorAddress,
}: {
    validatorAddress: SuiAddress;
}) {
    return (
        <Link
            to={`/stake/validator-details?address=${encodeURIComponent(
                validatorAddress
            )}`}
            className="flex no-underline flex-col py-3 px-3.75 box-border h-36 w-full rounded-2xl border hover:bg-sui/10 group border-solid border-gray-45 hover:border-sui/10 bg-transparent"
        >
            <div className="flex justify-between items-center mb-2">
                <ValidatorLogo
                    validatorAddress={validatorAddress}
                    size="subtitle"
                    iconSize="md"
                    stacked
                />
                <div className="text-gray-60 text-p1 opacity-0 group-hover:opacity-100">
                    <IconTooltip
                        tip="Annual Percentage Yield"
                        placement="top"
                    />
                </div>
            </div>
            <div className="flex-1">
                <div className="flex items-baseline gap-1 mt-1">
                    <Text variant="body" weight="semibold" color="gray-90">
                        test
                    </Text>

                    <Text variant="subtitle" weight="normal" color="gray-90">
                        SUI
                    </Text>
                </div>
            </div>
        </Link>
    );
}
