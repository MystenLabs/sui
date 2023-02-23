// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Copy16 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';

import { useCopyToClipboard } from '_src/ui/app/hooks/useCopyToClipboard';
import { Heading } from '_src/ui/app/shared/heading';

export type AccountProps = {
    address: string;
};

export function Account({ address }: AccountProps) {
    const copyCallback = useCopyToClipboard(address, {
        copySuccessMessage: 'Address copied',
    });
    return (
        <div className="flex flex-nowrap items-center border border-solid border-gray-60 rounded-lg p-5">
            <div className="flex-1">
                <Heading
                    mono
                    weight="semibold"
                    variant="heading6"
                    color="steel-dark"
                    leading="none"
                >
                    {formatAddress(address)}
                </Heading>
            </div>
            <Copy16
                onClick={copyCallback}
                className="transition text-base leading-none text-gray-60 active:text-gray-60 hover:text-hero-darkest cursor-pointer p1"
            />
        </div>
    );
}
