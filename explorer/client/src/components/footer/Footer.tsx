// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from 'react-router-dom';

import ExternalLink from '../external-link/ExternalLink';

import styles from './Footer.module.css';

function Footer() {
    return (
        <footer className={styles.footer}>
            <nav className={styles.links}>
                <Link to="/" id="homeBtn">
                    Home
                </Link>
                <ExternalLink href="https://sui.io/" label="Sui" />
                <ExternalLink
                    href="https://mystenlabs.com/"
                    label="Mysten Labs"
                />
                <ExternalLink
                    href="https://docs.sui.io/"
                    label="Developer Hub"
                />
            </nav>
        </footer>
    );
}

export default Footer;
