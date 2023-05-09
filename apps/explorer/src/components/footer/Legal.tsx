// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { legalLinks } from './footerLinks';

import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

export function LegalText() {
    return (
        <div className="flex justify-center md:justify-start">
            <Text color="steel-darker" variant="pSubtitleSmall/medium">
                &copy;
                {`${new Date().getFullYear()} Mysten Labs. All
  rights reserved.`}
            </Text>
        </div>
    );
}

export function LegalLinks() {
    return (
        <ul className="flex flex-col gap-3 md:flex-row md:gap-8">
            {legalLinks.map(({ title, href }) => (
                <li className="flex items-center justify-center" key={href}>
                    <Link variant="text" href={href}>
                        <Text
                            variant="subtitleSmall/medium"
                            color="steel-darker"
                        >
                            {title}
                        </Text>
                    </Link>
                </li>
            ))}
        </ul>
    );
}
