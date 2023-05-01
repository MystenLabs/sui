// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui, SuiLogoTxt } from '@mysten/icons';

import { type FooterItems, footerLinks } from './footerLinks';

import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

function FooterLinks({ links }: { links: FooterItems }) {
    return (
        <>
            {links.map(({ category, items }) => (
                <div
                    key={category}
                    className="flex flex-col gap-y-3.5 text-left"
                >
                    <Text variant="captionSmall/bold" color="gray-60">
                        {category}
                    </Text>
                    <ul className="flex flex-col gap-y-3.5">
                        {items.map(({ title, href }) => (
                            <li key={href}>
                                <Link variant="text" href={href}>
                                    <Text variant="body/medium" color="white">
                                        {title}
                                    </Text>
                                </Link>
                            </li>
                        ))}
                    </ul>
                </div>
            ))}
        </>
    );
}

function Footer() {
    return (
        <footer className="bg-gray-75 px-5 py-10 md:px-10 md:py-14">
            <nav className="mx-auto grid grid-cols-1 gap-8 md:mx-0 md:grid-cols-4 md:gap-10 xl:w-1/2">
                <div className="order-last mx-auto md:order-first md:mt-0">
                    <div className="h-full space-y-2 md:flex md:flex-col md:justify-between">
                        <div className="flex flex-row flex-nowrap items-center gap-1 text-white">
                            <Sui className="h-[26px] w-5" />
                            <SuiLogoTxt className="h-4 w-[26px]" />
                        </div>
                        <div className="mt-auto">
                            <Text
                                color="white"
                                variant="pSubtitleSmall/semibold"
                            >
                                &copy;
                                {`${new Date().getFullYear()} Sui. All
                                rights reserved.`}
                            </Text>
                        </div>
                    </div>
                </div>
                <div className="col-span-1 grid grid-cols-4 md:col-span-3">
                    <FooterLinks links={footerLinks} />
                </div>
            </nav>
        </footer>
    );
}

export default Footer;
