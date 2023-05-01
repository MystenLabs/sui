// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInitializedGuard } from '../../hooks';
import { Text } from '../../shared/text';
import SadCapy from './SadCapy.svg';

export function RestrictedPage() {
    useInitializedGuard(true);

    return (
        <div className="bg-sui/10 rounded-20 py-15 px-10 max-w-[400px] w-full text-center flex flex-col items-center gap-10">
            <SadCapy role="presentation" />
            <Text variant="pBody" color="steel-darker" weight="medium">
                Regrettably this service is not available to you. Applicable
                laws prohibit us from providing our services to your location at
                this time.
            </Text>
        </div>
    );
}
