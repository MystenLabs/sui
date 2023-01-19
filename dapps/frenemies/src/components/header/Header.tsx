// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton } from '@mysten/wallet-kit';

function Header() {
    return (
        <header className="py-5 bg-blue-700">
            <div className="mx-auto flex h-full items-end px-5">
                {/* Title + Description */}
                <div className="h-full w-auto">
                    <h1 className="mx-1">Sui Frenemies</h1>
                </div>

                {/* Connect button on the right */}
                <div className="w-full align-middle text-right">
                    <ConnectButton />
                </div>
            </div>
        </header>
    );
}

export default Header;
