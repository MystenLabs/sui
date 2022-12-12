// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '_app/shared/heading';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useMiddleEllipsis, useGetValidators, useFormatCoin } from '_hooks';

const TRUNCATE_MAX_LENGTH = 10;
const TRUNCATE_PREFIX_LENGTH = 6;
const APY_TOOLTIP = 'Annual Percentage Yield';

type ValidatorCardProp = {
    validatorAddress: string;
    accountAddress: string;
    stakeType: string;
    amount: number;
    coinType: string;
};

export function ValidatorCard({
    validatorAddress,
    accountAddress,
    amount,
    stakeType,
    coinType,
}: ValidatorCardProp) {
    const { validators, isLoading, isError } = useGetValidators(accountAddress);

    const validatorDataByAddress = validators.find(
        ({ address }) => address === validatorAddress
    );

    const truncatedAddress = useMiddleEllipsis(
        validatorDataByAddress?.address || null,
        TRUNCATE_MAX_LENGTH,
        TRUNCATE_PREFIX_LENGTH
    );

    if (isLoading) {
        return (
            <div className="p-2 w-full flex justify-center item-center h-full">
                <LoadingIndicator />
            </div>
        );
    }

    if (isError) {
        return (
            <div className="p-2">
                <Alert mode="warning">
                    <div className="mb-1 font-semibold">
                        Something went wrong
                    </div>
                </Alert>
            </div>
        );
    }

    return validatorDataByAddress ? (
        <div className="flex flex-col divide-y divide-solid divide-steel/20 divide-x-0 w-full">
            <div className="flex gap-2.5 py-3.5">
                <div className="mb-2 relative">
                    <ImageIcon
                        src={validatorDataByAddress.logo}
                        alt={validatorDataByAddress.name}
                    />
                </div>

                <div className="flex flex-col gap-1.5 capitalize">
                    <Text variant="body" weight="semibold" color="gray-90">
                        {validatorDataByAddress.name}
                    </Text>
                    <ExplorerLink
                        type={ExplorerLinkType.address}
                        address={validatorDataByAddress.address}
                        className="text-steel-dark no-underline font-mono font-medium "
                        showIcon={false}
                    >
                        {truncatedAddress}
                    </ExplorerLink>
                </div>
            </div>
            {validatorDataByAddress.apy && (
                <div className="py-3.5">
                    <ApyCard apy={validatorDataByAddress.apy} />
                </div>
            )}
            <div className="py-3.5">
                <StakeAmount
                    amount={amount}
                    coinType={coinType}
                    stakeType={stakeType}
                />
            </div>
        </div>
    ) : null;
}

function ApyCard({ apy }: { apy: number | string }) {
    return (
        <div className="flex items-center w-full justify-between">
            <div className="flex gap-0.5 items-center leading-none">
                <Text variant="body" weight="medium" color="steel-dark">
                    APY
                </Text>
                <div className="text-steel text-heading6 ">
                    <IconTooltip tip={`${APY_TOOLTIP}`} placement="top" />
                </div>
            </div>
            <Text variant="body" weight="medium" color="steel-darker">
                {apy === 'N/A' ? apy : `${apy}%`}
            </Text>
        </div>
    );
}

type StakeAmountProps = {
    amount: number;
    stakeType: string;
    coinType: string;
};
function StakeAmount({ amount, coinType, stakeType }: StakeAmountProps) {
    const [formatted, symbol] = useFormatCoin(Math.abs(amount), coinType);
    return (
        <div className="flex items-center w-full justify-between">
            <div className="flex gap-0.5 items-center leading-none">
                <Text variant="body" weight="medium" color="steel-dark">
                    {stakeType}
                </Text>
            </div>
            <div className="flex gap-0.5">
                <Heading variant="heading2" as="div">
                    {formatted}
                </Heading>
                <Text variant="body" weight="medium" color="steel-darker">
                    {symbol}
                </Text>
            </div>
        </div>
    );
}
