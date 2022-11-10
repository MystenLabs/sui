// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import { API_ENV_TO_INFO, API_ENV } from '_app/ApiProvider';
import Button from '_app/shared/button';
import { requestGas } from '_app/shared/faucet/actions';
import Icon, { SuiIcons } from '_components/icon';
import { useAppDispatch, useAppSelector } from '_hooks';
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
    const dispatch = useAppDispatch();
    const loading = useAppSelector(({ faucet }) => faucet.loading);
    return showFaucetRequestButton ? (
        <Button
            mode={mode}
            onClick={() => {
                dispatch(requestGas());
                trackEvent('RequestGas', {
                    props: { source: trackEventSource, networkName },
                });
            }}
            disabled={loading}
        >
            <Icon icon={SuiIcons.Download} className={cl(st.icon, st[mode])} />
            Request {networkName} SUI Tokens
        </Button>
    ) : null;
}

export default FaucetRequestButton;
