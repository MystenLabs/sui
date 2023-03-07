// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress, type SuiAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { useMemo } from 'react';

import { useSystemState } from '../useSystemState';
import { Heading } from '_app/shared/heading';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';

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
    const { data, isLoading } = useSystemState();

    const validatorMeta = useMemo(() => {
        if (!data) return null;

        const validator = data.validators.active_validators.find(
            ({ metadata }) => metadata.sui_address === validatorAddress
        );
        if (!validator) return null;

        return {
            name: validator.metadata.name,
            logo: validator.metadata.image_url,
        };
    }, [validatorAddress, data]);

    if (isLoading) {
        return <div className="flex justify-center items-center">...</div>;
    }

    return validatorMeta ? (
        <div
            className={cl(
                ['w-full flex justify-start  font-semibold'],
                stacked ? 'flex-col items-start' : 'flex-row items-center',
                isTitle ? 'gap-2.5' : 'gap-2'
            )}
        >
            <ImageIcon
                src={validatorMeta.logo}
                label={validatorMeta.name}
                fallback={validatorMeta.name}
                size={iconSize}
                circle
            />
            <div className="flex flex-col gap-1.5">
                {isTitle ? (
                    <Heading as="h4" variant="heading4" color="steel-darker">
                        {validatorMeta.name}
                    </Heading>
                ) : (
                    <Text color="gray-90" variant={size} weight="semibold">
                        {validatorMeta.name}
                    </Text>
                )}
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
    ) : null;
}
