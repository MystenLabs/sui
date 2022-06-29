// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import { ReactComponent as SuiLogo } from '../../assets/Sui Logo.svg';
import NetworkSelect from '../network/Network';
import Search from '../search/Search';

import styles from './Header.module.css';

const Header = () => {
    return (
        <header>
            <Link id="homeBtn" className={styles.suititle} to="/">
                <SuiLogo />
            </Link>

            <div className={styles.search}>
                <Search />
            </div>

            <div className={styles.networkselect}>
                <NetworkSelect />
            </div>
        </header>
    );
};

export default Header;
