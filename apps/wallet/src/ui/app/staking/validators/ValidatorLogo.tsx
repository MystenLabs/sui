// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useGetSystemState } from '@mysten/core';
import { formatAddress, type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { Heading } from '_app/shared/heading';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { Badge } from '_src/ui/app/shared/Badge';

interface ValidatorLogoProps {
    validatorAddress: SuiAddress;
    showAddress?: boolean;
    stacked?: boolean;
    isTitle?: boolean;
    size: 'body' | 'subtitle';
    iconSize: 'sm' | 'md';
    showActiveStatus?: boolean;
    activeEpoch?: string;
}

export function ValidatorLogo({
    validatorAddress,
    showAddress,
    iconSize,
    isTitle,
    size,
    stacked,
    showActiveStatus = false,
    activeEpoch,
}: ValidatorLogoProps) {
    const { data, isLoading } = useGetSystemState();

    const validatorMeta = useMemo(() => {
        if (!data) return null;

        return (
            data.activeValidators.find(
                (validator) => validator.suiAddress === validatorAddress
            ) || null
        );
    }, [validatorAddress, data]);

    const stakingPoolActivationEpoch = Number(
        validatorMeta?.stakingPoolActivationEpoch || 0
    );
    const currentEpoch = Number(data?.epoch || 0);

    // flag as new validator if the validator was activated in the last epoch
    // for genesis validators, this will be false
    const newValidator =
        currentEpoch - stakingPoolActivationEpoch <= 1 && currentEpoch !== 0;

    // flag if the validator is at risk of being removed from the active set
    const isAtRisk = data?.atRiskValidators.some(
        (item) => item[0] === validatorAddress
    );

    if (isLoading) {
        return <div className="flex justify-center items-center">...</div>;
    }
    // for inactive validators, show the epoch number
    const fallBackText = activeEpoch
        ? `Staked ${Number(data?.epoch) - Number(activeEpoch)} epochs ago`
        : '';
    const validatorName = validatorMeta?.name || fallBackText;

    return (
        <div
            className={cl(
                'w-full flex justify-start font-semibold',
                stacked ? 'flex-col items-start' : 'flex-row items-center',
                isTitle ? 'gap-2.5' : 'gap-2'
            )}
        >
            <ImageIcon
                src={validatorMeta?.imageUrl || null}
                label={validatorMeta?.name || ''}
                fallback={validatorMeta?.name || ''}
                size={iconSize}
                circle
            />
            <div className="flex flex-col gap-1.5 overflow-hidden">
                <div className="flex">
                    {isTitle ? (
                        <Heading
                            as="h4"
                            variant="heading4"
                            color="steel-darker"
                            truncate
                        >
                            {validatorName}
                        </Heading>
                    ) : (
                        <div className="line-clamp-2 break-all">
                            <Text
                                color="gray-90"
                                variant={size}
                                weight="semibold"
                            >
                                {validatorName}
                            </Text>
                        </div>
                    )}

                    {showActiveStatus && (
                        <div className="ml-1 flex gap-1">
                            {newValidator && (
                                <Badge label="New" variant="success" />
                            )}
                            {isAtRisk && (
                                <Badge label="At Risk" variant="warning" />
                            )}
                        </div>
                    )}
                </div>
                {showAddress && (
                    <ExplorerLink
                        type={ExplorerLinkType.validator}
                        validator={validatorAddress}
                        showIcon={false}
                        className="text-steel-dark no-underline text-body font-mono"
                    >
                        {formatAddress(validatorAddress)}
                    </ExplorerLink>
                )}
            </div>
        </div>
    );
}
