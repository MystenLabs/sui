// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as SuiLogoIcon } from '../../assets/Sui Logo.svg';

import styles from './Footer.module.css';

import { Link } from '~/ui/Link';

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
                            <Link href="https://medium.com/mysten-labs">
                                Blog
                            </Link>
                        </li>
                        <li>
                            <Link href="https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf">
                                Whitepaper
                            </Link>
                        </li>
                    </ul>
                </div>
                <div>
                    <h6>Build</h6>
                    <ul>
                        <li>
                            <Link href="https://docs.sui.io/">Docs</Link>
                        </li>
                        <li>
                            <Link href="https://github.com/MystenLabs">
                                GitHub
                            </Link>
                        </li>
                        <li>
                            <Link href="https://discord.gg/sui">Discord</Link>
                        </li>
                    </ul>
                </div>
                <div>
                    <h6>Follow</h6>
                    <ul>
                        <li>
                            <Link href="https://mystenlabs.com/#community">
                                Press
                            </Link>
                        </li>
                        <li>
                            <Link href="https://twitter.com/SuiNetwork">
                                Twitter
                            </Link>
                        </li>
                        <li>
                            <Link href="https://www.linkedin.com/company/mysten-labs/">
                                LinkedIn
                            </Link>
                        </li>
                    </ul>
                </div>
                <div>
                    <h6>Legal</h6>
                    <ul>
                        <li>
                            <Link href="https://mystenlabs.com/legal?content=terms">
                                Terms & Conditions
                            </Link>
                        </li>
                        <li>
                            <Link href="https://mystenlabs.com/legal?content=privacy">
                                Privacy Policy
                            </Link>
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
