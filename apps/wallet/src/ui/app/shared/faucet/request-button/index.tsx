// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { API_ENV_TO_INFO } from '_app/ApiProvider';
import Button from '_app/shared/button';
import { requestGas } from '_app/shared/faucet/actions';
import Icon, { SuiIcons } from '_components/icon';
import { useAppDispatch, useAppSelector } from '_hooks';

import type { ButtonProps } from '_app/shared/button';

import st from './RequestButton.module.scss';

type FaucetRequestButtonProps = {
    mode?: ButtonProps['mode'];
};

function FaucetRequestButton({ mode = 'primary' }: FaucetRequestButtonProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const networkName = API_ENV_TO_INFO[network].name;
    const dispatch = useAppDispatch();
    const loading = useAppSelector(({ faucet }) => faucet.loading);
    return (
        <Button
            mode={mode}
            onClick={() => {
                dispatch(requestGas());
            }}
            disabled={loading}
        >
            <Icon icon={SuiIcons.Download} className={st.icon} />
            Request {networkName} SUI Tokens
        </Button>
    );
}

export default FaucetRequestButton;
