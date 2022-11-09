// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as SuiLogo } from '../../assets/Sui Logo.svg';
import NetworkSelect from '../network/Network';
import Search from '../search/Search';

import styles from './Header.module.css';

import { LinkWithQuery } from '~/ui/utils/LinkWithQuery';

function Header() {
    return (
        <header>
            <LinkWithQuery
                id="homeBtn"
                data-testid="nav-logo-button"
                className={styles.suititle}
                to="/"
            >
                <SuiLogo />
            </LinkWithQuery>

            <div className={styles.search}>
                <Search />
            </div>

            <div className={styles.networkselect}>
                <NetworkSelect />
            </div>
        </header>
    );
}

export default Header;
