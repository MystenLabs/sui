// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight16 } from '@mysten/icons';
import cl from 'classnames';

import ExternalLink from '_components/external-link';
import { trackEvent } from '_shared/plausible';

import type { ButtonProps } from '_app/shared/button';

//TODO: Remove delete this file after testnet
const TESTNET_DISCORD_LINK =
    'https://discord.com/channels/916379725201563759/1037811694564560966';

export function FaucetDiscordLink({
    mode = 'primary',
    trackEventSource,
}: {
    mode?: ButtonProps['mode'];
    trackEventSource: 'home' | 'settings';
}) {
    return (
        <ExternalLink
            className={cl(
                'flex flex-nowrap max-w-full items-center justify-center rounded-xl px-5 py-3.5 gap-2 no-underline font-semibold text-body',
                mode === 'primary'
                    ? 'bg-hero-dark text-white hover:bg-hero'
                    : 'bg-white text-steel border-solid border-color-steel border hover:border-color-steel-dark hover:text-steel-dark'
            )}
            href={TESTNET_DISCORD_LINK}
            showIcon={false}
            onClick={() =>
                trackEvent('DiscordRequestSUIToken', {
                    props: { source: trackEventSource },
                })
            }
        >
            Request Testnet SUI on Discord
            <ArrowUpRight16
                className={cl(mode === 'primary' ? 'text-white' : 'text-steel')}
            />
        </ExternalLink>
    );
}
