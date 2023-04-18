// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransferObject16 } from '@mysten/icons';

import { Text } from '_src/ui/app/shared/text';

export function NoActivityCard() {
    return (
        <div className="flex flex-col gap-4 justify-center items-center text-center h-full px-10">
            <TransferObject16 className="text-gray-45 text-3xl" />
            <Text variant="pBody" weight="medium" color="steel">
                When available, your Sui network transactions will show up here.
            </Text>
        </div>
    );
}
