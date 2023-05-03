// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as MystenLabsRed } from '../../assets/MystenLabs_Red.svg';
import { LegalLinks, LegalText } from './Legal';
import { footerLinks, socialLinks } from './footerLinks';

import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

function FooterLinks() {
    return (
        <div className="flex flex-col items-center justify-center gap-6 md:flex-row md:justify-end">
            <ul className="flex gap-6 md:flex-row">
                {footerLinks.map(({ title, href }) => (
                    <li key={href}>
                        <Link variant="text" href={href}>
                            <Text variant="body/medium" color="steel-darker">
                                {title}
                            </Text>
                        </Link>
                    </li>
                ))}
            </ul>

            <ul className="flex justify-center gap-6">
                {socialLinks.map(({ children, href }) => (
                    <li key={href}>
                        <Link variant="text" color="steel-darker" href={href}>
                            <div className="mt-2">{children}</div>
                        </Link>
                    </li>
                ))}
            </ul>
        </div>
    );
}

function Footer() {
    return (
        <footer className="bg-gray-40 px-5 py-10 md:px-10 md:py-14">
            <nav className="flex flex-col justify-center gap-4 divide-y divide-solid divide-gray-45 md:gap-7.5">
                <div className="flex flex-col-reverse items-center gap-7.5 md:flex-row md:justify-between ">
                    <div className="hidden self-center text-hero-dark md:flex md:self-start">
                        <MystenLabsRed />
                    </div>
                    <div>
                        <FooterLinks />
                    </div>
                </div>
                <div className="flex flex-col-reverse justify-center gap-3 pt-3 md:flex-row md:justify-between">
                    <LegalText />
                    <LegalLinks />
                </div>
            </nav>
            <div className="mt-4 flex justify-center border-t border-solid border-gray-45 pt-5 text-hero-dark md:hidden md:self-start">
                <MystenLabsRed />
            </div>
        </footer>
    );
}

export default Footer;
