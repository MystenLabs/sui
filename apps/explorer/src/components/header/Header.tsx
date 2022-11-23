// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as SuiLogo } from '../../assets/Sui Logo.svg';
import NetworkSelect from '../network/Network';
import Search from '../search/Search';

import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

function Header() {
    return (
        <header className="bg-headerNav h-header">
            <div className="flex items-center max-w-[1440px] mx-auto px-5 h-full">
                <div className="mr-8">
                    <LinkWithQuery data-testid="nav-logo-button" to="/">
                        <SuiLogo />
                    </LinkWithQuery>
                </div>

                <div className="flex-1">
                    <Search />
                </div>

                <div className="ml-2">
                    <NetworkSelect />
                </div>
            </div>
        </header>
    );
}

export default Header;
