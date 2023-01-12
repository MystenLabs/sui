// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { useGetValidatorMetaData } from '../useGetDelegatedStake';
import { getName } from '../usePendingDelegation';
import { Heading } from '_app/shared/heading';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import { useMiddleEllipsis } from '_hooks';

const TRUNCATE_MAX_LENGTH = 8;
const TRUNCATE_PREFIX_LENGTH = 4;

interface ValidatorLogoProps {
    validatorAddress: SuiAddress;
    showAddress?: boolean;
    stacked?: boolean;
    isTitle?: boolean;
    size: 'body' | 'subtitle';
    iconSize: 'sm' | 'md';
}

export function ValidatorLogo({
    validatorAddress,
    showAddress,
    iconSize,
    isTitle,
    size,
    stacked,
}: ValidatorLogoProps) {
    const { data: validatorsData, isLoading } = useGetValidatorMetaData();
    const truncatedAddress = useMiddleEllipsis(
        validatorAddress,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    const validatorName = useMemo(() => {
        if (!validatorsData) return null;

        const validator = validatorsData.find(
            ({ sui_address }) => sui_address === validatorAddress
        );
        if (!validator) return null;

        return getName(validator.name);
    }, [validatorAddress, validatorsData]);

    const logo = null;

    if (isLoading) {
        return <div className="flex justify-center items-center">...</div>;
    }

    return validatorName ? (
        <div
            className={cl(
                ['w-full flex justify-start  font-semibold'],
                stacked ? 'flex-col items-start' : 'flex-row items-center',
                isTitle ? 'gap-2.5' : 'gap-2'
            )}
        >
            <ImageIcon
                src={logo}
                label={validatorName}
                fallback={validatorName}
                size={iconSize}
                circle
            />
            <div className="flex flex-col gap-1.5">
                {isTitle ? (
                    <Heading as="h4" variant="heading4" color="steel-darker">
                        {validatorName}
                    </Heading>
                ) : (
                    <Text color="gray-90" variant={size} weight="semibold">
                        {validatorName}
                    </Text>
                )}
                {showAddress && (
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={validatorAddress}
                        showIcon={false}
                        className="text-steel-dark no-underline text-body font-mono"
                    >
                        {truncatedAddress}
                    </ExplorerLink>
                )}
            </div>
        </div>
    ) : null;
}
