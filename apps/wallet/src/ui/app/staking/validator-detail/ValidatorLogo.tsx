// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { useGetValidatorMetaData } from '../useGetValidatorsData';
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

    const validatorData = useMemo(() => {
        if (!validatorsData) return null;

        const validator = validatorsData.find(
            ({ sui_address }) => sui_address === validatorAddress
        );

        if (!validator) return null;

        return {
            name: getName(validator.name),
            // TODO: add once we have the logo in the metadata
            logo: null,
        };
    }, [validatorAddress, validatorsData]);

    if (isLoading) {
        return <div className="flex justify-center items-center">...</div>;
    }

    return validatorData ? (
        <div
            className={cl(
                ['w-full flex justify-start  font-semibold'],
                stacked ? 'flex-col items-start' : 'flex-row items-center',
                isTitle ? 'gap-2.5' : 'gap-2'
            )}
        >
            <ImageIcon
                src={validatorData.logo}
                alt={validatorData.name}
                size={iconSize}
                circle
            />
            <div className="flex flex-col gap-1.5">
                {isTitle ? (
                    <Heading as="h4" variant="heading4" color="steel-darker">
                        {validatorData.name}
                    </Heading>
                ) : (
                    <Text color="gray-90" variant={size} weight="semibold">
                        {validatorData.name}
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
