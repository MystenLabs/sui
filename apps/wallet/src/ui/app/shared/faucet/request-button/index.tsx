// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight16 } from '@mysten/icons';
import cl from 'classnames';
import { toast } from 'react-hot-toast';

import FaucetMessageInfo from '../message-info';
import { useFaucetMutation } from '../useFaucetMutation';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import Button from '_app/shared/button';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { trackEvent } from '_shared/plausible';

import type { ButtonProps } from '_app/shared/button';

import st from './RequestButton.module.scss';

//TODO:  Remove post testnet
const TESTNET_DISCORD_LINK =
    'https://discord.com/channels/916379725201563759/1037811694564560966';

type FaucetRequestButtonProps = {
    mode?: ButtonProps['mode'];
    trackEventSource: 'home' | 'settings';
};

function FaucetDiscordLink({
    mode = 'primary',
    trackEventSource,
}: {
    mode?: ButtonProps['mode'];
    trackEventSource: 'home' | 'settings';
}) {
    return (
        <ExternalLink
            className={cl(
                'flex flex-nowrap max-w-full items-center justify-center rounded-xl px-5 py-3 gap-2 no-underline font-medium text-body',
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

function FaucetRequestButton({
    mode = 'primary',
    trackEventSource,
}: FaucetRequestButtonProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const networkName = API_ENV_TO_INFO[network].name.replace(/sui\s*/gi, '');
    const mutation = useFaucetMutation();

    //TODO: remove this TestNet check after testnet
    return mutation.enabled || network === 'testNet' ? (
        mutation.enabled ? (
            <Button
                mode={mode}
                onClick={() => {
                    mutation.enabled &&
                        toast.promise(mutation.mutateAsync(), {
                            loading: <FaucetMessageInfo loading />,
                            success: (totalReceived) => (
                                <FaucetMessageInfo
                                    totalReceived={totalReceived}
                                />
                            ),
                            error: (error) => (
                                <FaucetMessageInfo error={error.message} />
                            ),
                        });
                    trackEvent('RequestGas', {
                        props: { source: trackEventSource, networkName },
                    });
                }}
                disabled={mutation.isMutating}
            >
                <Icon
                    icon={SuiIcons.Download}
                    className={cl(st.icon, st[mode])}
                />
                Request {networkName} SUI Tokens
            </Button>
        ) : (
            <FaucetDiscordLink
                mode={mode}
                trackEventSource={trackEventSource}
            />
        )
    ) : null;
}

export default FaucetRequestButton;
