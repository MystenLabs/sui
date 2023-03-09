// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress } from '@mysten/sui.js';

import { useNextMenuUrl } from '../hooks';
import { Link } from '_src/ui/app/shared/Link';

export type AccountActionsProps = {
    accountAddress: SuiAddress;
};

export function AccountActions({ accountAddress }: AccountActionsProps) {
    const exportAccountUrl = useNextMenuUrl(true, `/export/${accountAddress}`);
    return (
        <div className="flex flex-row flex-nowrap items-center flex-1">
            <div>
                <Link
                    text="Export Private Key"
                    to={exportAccountUrl}
                    color="heroDark"
                    weight="medium"
                />
            </div>
        </div>
    );
}
