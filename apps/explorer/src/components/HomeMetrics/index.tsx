// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type ValidatorsFields } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from '~/hooks/useAppsBackend';
import { useGetObject } from '~/hooks/useGetObject';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Text } from '~/ui/Text';

interface MetricProps {
    label: string;
    value?: string | number | null;
}

const numberFormatter = new Intl.NumberFormat(undefined);
function Metric({ label, value }: MetricProps) {
    return (
        <div className="flex items-center justify-between">
            <Text variant="caption/semibold" color="steel-darker">
                {label}
            </Text>
            <Heading variant="heading6/semibold" color="steel-darker">
                {typeof value === 'number'
                    ? numberFormatter.format(value)
                    : value ?? '--'}
            </Heading>
        </div>
    );
}

interface CountsResponse {
    addresses: number;
    objects: number;
    packages: number;
    transactions: number;
}

interface TPSResponse {
    tps: number;
}

function roundFloat(number: number, decimals: number) {
    return parseFloat(number.toFixed(decimals));
}

export function HomeMetrics() {
    const request = useAppsBackend();
    const { data: systemData } = useGetObject('0x5');
    const systemObject =
        systemData &&
        is(systemData.details, SuiObject) &&
        systemData.details.data.dataType === 'moveObject'
            ? (systemData.details.data.fields as ValidatorsFields)
            : null;

    const { data: countsData } = useQuery(['home', 'counts'], () =>
        request<CountsResponse>('counts', { network: 'PRIVATE_TESTNET' })
    );

    const { data: tpsData } = useQuery(['home', 'tps'], () =>
        request<TPSResponse>('tps', { network: 'PRIVATE_TESTNET' })
    );

    return (
        <Card>
            <div className="grid grid-cols-1 space-y-3 lg:grid-cols-2 lg:gap-10 lg:space-y-0">
                <div className="space-y-3">
                    <Metric label="Total Objects" value={countsData?.objects} />
                    <Metric
                        label="Total Packages"
                        value={countsData?.packages}
                    />
                    <Metric
                        label="Total Transactions"
                        value={countsData?.transactions}
                    />
                </div>
                <div className="space-y-3">
                    <Metric label="Current Epoch" value={systemObject?.epoch} />
                    <Metric
                        label="Current Validators"
                        value={
                            systemObject?.validators.fields.active_validators
                                .length
                        }
                    />
                    <Metric
                        label="Current TPS"
                        value={tpsData?.tps ? roundFloat(tpsData.tps, 2) : null}
                    />
                </div>
            </div>
        </Card>
    );
}
