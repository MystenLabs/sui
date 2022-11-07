// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as SuiLogoIcon } from '../../assets/Sui Logo.svg';
import ExternalLink from '../external-link/ExternalLink';

import styles from './Footer.module.css';

function Footer() {
    return (
        <footer>
            <nav className={styles.links}>
                <div className={styles.logodesktop}>
                    <SuiLogoIcon />
                    <div className={styles.copyright}>
                        <div>&copy;2022 Sui</div>
                        <div>All rights reserved</div>
                    </div>
                </div>

                <div>
                    <h6>Read</h6>
                    <ul>
                        <li>
                            <ExternalLink
                                href="https://medium.com/mysten-labs"
                                label="Blog"
                            />
                        </li>
                        <li>
                            <ExternalLink
                                href="https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf"
                                label="Whitepaper"
                            />
                        </li>
                    </ul>
                </div>
                <div>
                    <h6>Build</h6>
                    <ul>
                        <li>
                            <ExternalLink
                                href="https://docs.sui.io/"
                                label="Docs"
                            />
                        </li>
                        <li>
                            <ExternalLink
                                href="https://github.com/MystenLabs"
                                label="GitHub"
                            />
                        </li>
                        <li>
                            <ExternalLink
                                href="https://discord.gg/sui"
                                label="Discord"
                            />
                        </li>
                    </ul>
                </div>
                <div>
                    <h6>Follow</h6>
                    <ul>
                        <li>
                            <ExternalLink
                                href="https://mystenlabs.com/#community"
                                label="Press"
                            />
                        </li>
                        <li>
                            <ExternalLink
                                href="https://twitter.com/SuiNetwork"
                                label="Twitter"
                            />
                        </li>
                        <li>
                            <ExternalLink
                                href="https://www.linkedin.com/company/mysten-labs/"
                                label="LinkedIn"
                            />
                        </li>
                    </ul>
                </div>
            </nav>
            <div className={styles.logomobile}>
                <SuiLogoIcon />
                <div className={styles.copyright}>
                    <div>&copy;2022 Sui. All rights reserved.</div>
                </div>
            </div>
        </footer>
    );
}

export default Footer;
