// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SocialDiscord24,
    SocialLinkedin24,
    SocialTwitter24,
} from '@mysten/icons';

import { ReactComponent as SuiWordmark } from '../../assets/SuiWordmark.svg';

import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

function FooterLinks() {
    return (
        <ul className="flex gap-8">
            <li>
                <Link variant="text" href="https://medium.com/mysten-labs">
                    <Text variant="body/medium" color="steel-darker">
                        Blog
                    </Text>
                </Link>
            </li>
            <li>
                <Link
                    variant="text"
                    href="https://github.com/MystenLabs/sui/blob/main/doc/paper/sui.pdf"
                >
                    <Text variant="body/medium" color="steel-darker">
                        Whitepaper
                    </Text>
                </Link>
            </li>
            <li>
                <Link variant="text" href="https://mystenlabs.com/#community">
                    <Text variant="body/medium" color="steel-darker">
                        Press
                    </Text>
                </Link>
            </li>
            <li>
                <Link variant="text" href="https://docs.sui.io/">
                    <Text variant="body/medium" color="steel-darker">
                        Docs
                    </Text>
                </Link>
            </li>
            <li>
                <Link variant="text" href="https://github.com/MystenLabs">
                    <Text variant="body/medium" color="steel-darker">
                        GitHub
                    </Text>
                </Link>
            </li>
            <li>
                <Link variant="text" href="https://discord.gg/sui">
                    <SocialDiscord24 />
                </Link>
            </li>
            <li>
                <Link variant="text" href="https://twitter.com/SuiNetwork">
                    <SocialTwitter24 />
                </Link>
            </li>
            <li>
                <Link
                    variant="text"
                    href="https://www.linkedin.com/company/mysten-labs/"
                >
                    <SocialLinkedin24 />
                </Link>
            </li>
        </ul>
    );
}

function Footer() {
    return (
        <footer className="bg-gray-40 px-5 py-10 md:px-10 md:py-14">
            <nav className="flex flex-col gap-7.5">
                <div className="flex w-full flex-col items-center justify-between gap-7.5 md:flex-row">
                    <div className="flex gap-2 text-hero-dark">
                        <SuiWordmark />
                    </div>
                    <FooterLinks />
                </div>

                <div className="h-[1px] w-full bg-gray-45" />
                <div className="flex w-full items-center justify-between">
                    <div className="h-full space-y-2">
                        <Text
                            color="steel-darker"
                            variant="pSubtitleSmall/medium"
                        >
                            &copy;
                            {`${new Date().getFullYear()} Sui. All
                                rights reserved.`}
                        </Text>
                    </div>
                    <ul className="flex gap-2">
                        <li>
                            <Link
                                variant="text"
                                href="https://mystenlabs.com/legal?content=terms"
                            >
                                <Text
                                    variant="pSubtitleSmall/medium"
                                    color="steel-darker"
                                >
                                    Terms & Conditions
                                </Text>
                            </Link>
                        </li>
                        <li>
                            <Link
                                variant="text"
                                href="https://mystenlabs.com/legal?content=privacy"
                            >
                                <Text
                                    variant="pSubtitleSmall/medium"
                                    color="steel-darker"
                                >
                                    Privacy Policy
                                </Text>
                            </Link>
                        </li>
                    </ul>
                </div>
            </nav>
        </footer>
    );
}

export default Footer;
