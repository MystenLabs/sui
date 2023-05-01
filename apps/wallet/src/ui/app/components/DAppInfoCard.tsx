// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight12 } from '@mysten/icons';
import { type SuiAddress } from '@mysten/sui.js';
import { cx } from 'class-variance-authority';
import { useMemo } from 'react';

import { Link } from '../shared/Link';
import { Heading } from '../shared/heading';
import { Text } from '../shared/text';
import { AccountAddress } from './AccountAddress';
import { SummaryCard } from './SummaryCard';

export type DAppInfoCardProps = {
    name: string;
    url: string;
    iconUrl?: string;
    connectedAddress?: SuiAddress;
};
export function DAppInfoCard({
    name,
    url,
    iconUrl,
    connectedAddress,
}: DAppInfoCardProps) {
    const hostname = useMemo(() => {
        try {
            return new URL(url).hostname;
        } catch (e) {
            // do nothing
        }
        return url;
    }, [url]);
    return (
        <SummaryCard
            showDivider
            body={
                <>
                    <div className="flex flex-row flex-nowrap items-center gap-3.75 pb-3">
                        <div className="flex items-stretch h-15 w-15 rounded-full overflow-hidden bg-steel/20 shrink-0 grow-0">
                            {iconUrl ? (
                                <img
                                    className="flex-1"
                                    src={iconUrl}
                                    alt={name}
                                />
                            ) : null}
                        </div>
                        <div className="flex flex-col flex-nowrap gap-1">
                            <Heading
                                variant="heading4"
                                weight="semibold"
                                color="gray-100"
                            >
                                {name}
                            </Heading>
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-dark"
                            >
                                {hostname}
                            </Text>
                        </div>
                    </div>
                    <div
                        className={cx(
                            'flex justify-start pt-3',
                            connectedAddress ? 'pb-3' : ''
                        )}
                    >
                        <div>
                            <Link
                                href={url}
                                title={name}
                                text="View Website"
                                after={<ArrowUpRight12 />}
                                color="suiDark"
                                weight="medium"
                            />
                        </div>
                    </div>
                    {connectedAddress ? (
                        <div className="flex flex-nowrap flex-row items-center pt-3 gap-3">
                            <Text
                                variant="bodySmall"
                                weight="medium"
                                color="steel-darker"
                                truncate
                            >
                                Connected account
                            </Text>
                            <div className="flex-1" />
                            <AccountAddress
                                copyable
                                address={connectedAddress}
                            />
                        </div>
                    ) : null}
                </>
            }
        />
    );
}
