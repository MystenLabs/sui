// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import { Text } from '../../shared/text';
import Icon, { SuiIcons } from '_components/icon';
import { API_ENV } from '_src/shared/api-env';

const networkNames: Record<API_ENV, string> = {
    [API_ENV.local]: 'Local',
    [API_ENV.testNet]: 'Testnet',
    [API_ENV.devNet]: 'Devnet',
    [API_ENV.customRPC]: 'Custom RPC',
};

type LogoProps = {
    networkName?: API_ENV;
};

const Logo = ({ networkName }: LogoProps) => {
    return (
        <div className="inline-flex flex-nowrap items-center text-2xl">
            <Icon icon={SuiIcons.SuiLogoIcon} />
            <div className={cl('flex flex-col', { 'mb-2': !!networkName })}>
                <Icon icon={SuiIcons.SuiLogoTxt} />
                {networkName && (
                    <div className="-mt-2 ml-0.5">
                        <Text variant="subtitleSmallExtra">
                            {networkNames[networkName]}
                        </Text>
                    </div>
                )}
            </div>
        </div>
    );
};

export default Logo;
