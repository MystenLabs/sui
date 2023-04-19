// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetSystemState } from '@mysten/core';

import { RingChart } from '~/ui/RingChart';

export function ValidatorStatus() {
    const { data } = useGetSystemState();
    if (!data) return null;

    const nextEpoch = Number(data.epoch || 0) + 1;

    return (
        <RingChart
            title={`Validators in Epoch ${nextEpoch}`}
            suffix="validators"
            data={[
                {
                    value: data.activeValidators.length,
                    label: 'Active',
                    color: '#589AEA',
                },
                {
                    value: Number(data.pendingActiveValidatorsSize ?? 0),
                    label: 'New',
                    color: '#F2BD24',
                },
                {
                    value: data.atRiskValidators.length,
                    label: 'At Risk',
                    color: '#FF794B',
                },
            ]}
        />
    );
}
