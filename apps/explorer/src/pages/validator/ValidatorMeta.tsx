// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { Validator } from '~/pages/validator/ValidatorDataTypes';

import { DescriptionList, DescriptionItem } from '~/ui/DescriptionList';
import { Heading } from '~/ui/Heading';
import { ImageIcon } from '~/ui/ImageIcon';
import { AddressLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';

type ValidatorMetaProps = {
    validatorData: Validator;
};

export function ValidatorMeta({ validatorData }: ValidatorMetaProps) {
    const validatorName = useMemo(() => {
        return getName(validatorData.fields.metadata.fields.name);
    }, [validatorData]);

    const logo = null;

    const validatorPublicKey = useMemo(
        () =>
            new Base64DataBuffer(
                new Uint8Array(
                    validatorData.fields.metadata.fields.pubkey_bytes
                )
            ).toString(),
        [validatorData]
    );

    return (
        <>
            <div className="flex basis-full gap-5 border-r border-solid border-transparent border-r-gray-45 capitalize md:mr-7.5 md:basis-1/4">
                <ImageIcon src={logo} alt={validatorName} size="xl" />
                <div className="mt-1 flex flex-col gap-2.5 pl-2 md:gap-3.5">
                    <Heading as="h1" variant="heading2/bold" color="gray-100">
                        {validatorName}
                    </Heading>
                </div>
            </div>
            <div className="basis-full break-all md:basis-2/3 ">
                <DescriptionItem title="Address">
                    <AddressLink
                        address={
                            validatorData.fields.metadata.fields.sui_address
                        }
                        noTruncate
                    />
                </DescriptionItem>
                <DescriptionList>
                    <DescriptionItem title="Public Key">
                        <Text variant="p1/medium" color="gray-90">
                            {validatorPublicKey}
                        </Text>
                    </DescriptionItem>
                </DescriptionList>
            </div>
        </>
    );
}
