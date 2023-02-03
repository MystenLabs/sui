// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { toast } from 'react-hot-toast';

import FaucetMessageInfo from '../message-info';
import { useFaucetMutation } from '../useFaucetMutation';
import { FaucetDiscordLink } from './FaucetDiscordLink';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { trackEvent } from '_shared/plausible';

import type { ButtonProps } from '_app/shared/button';

import st from './RequestButton.module.scss';

type FaucetRequestButtonProps = {
    mode?: ButtonProps['mode'];
    trackEventSource: 'home' | 'settings';
};

function FaucetRequestButton({
    mode = 'primary',
    trackEventSource,
}: FaucetRequestButtonProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const networkName = API_ENV_TO_INFO[network].name.replace(/sui\s*/gi, '');
    const mutation = useFaucetMutation();

    //TODO: remove this TestNet check after testnet
    if (network === 'testNet') {
        return (
            <FaucetDiscordLink
                mode={mode}
                trackEventSource={trackEventSource}
            />
        );
    }

    return mutation.enabled ? (
        <Button
            mode={mode}
            onClick={() => {
                toast.promise(mutation.mutateAsync(), {
                    loading: <FaucetMessageInfo loading />,
                    success: (totalReceived) => (
                        <FaucetMessageInfo totalReceived={totalReceived} />
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
            <Icon icon={SuiIcons.Download} className={cl(st.icon, st[mode])} />
            Request {networkName} SUI Tokens
        </Button>
    ) : null;
}

export default FaucetRequestButton;
