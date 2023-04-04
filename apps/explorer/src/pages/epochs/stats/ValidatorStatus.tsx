// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetSystemObject } from '~/hooks/useGetObject';
import { RingChart } from '~/ui/RingChart';

export function ValidatorStatus() {
    const { data } = useGetSystemObject();
    if (!data) return null;
    return (
        <RingChart
            title="Validators in Next Epoch"
            suffix="validators"
            data={[
                {
                    value: data.activeValidators.length,
                    label: 'Active',
                    color: '#589AEA',
                },
                {
                    value: +(data.pendingActiveValidatorsSize ?? 0),
                    label: 'New',
                    color: '#6FBCF0',
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
