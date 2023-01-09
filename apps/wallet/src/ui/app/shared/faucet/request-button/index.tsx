// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { toast } from 'react-hot-toast';

import FaucetMessageInfo from '../message-info';
import { useFaucetMutation, useIsFaucetMutating } from '../useFaucetMutation';
import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
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
    const networkName = API_ENV_TO_INFO[network].name;
    const showFaucetRequestButton = API_ENV.customRPC !== network;
    const mutation = useFaucetMutation();
    const isMutating = useIsFaucetMutating();
    return showFaucetRequestButton ? (
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
            disabled={mutation.isLoading || isMutating}
        >
            <Icon icon={SuiIcons.Download} className={cl(st.icon, st[mode])} />
            Request {networkName} SUI Tokens
        </Button>
    ) : null;
}

export default FaucetRequestButton;
