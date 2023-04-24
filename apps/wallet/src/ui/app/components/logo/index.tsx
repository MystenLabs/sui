// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui, SuiLogoTxt } from '@mysten/icons';
import cl from 'classnames';

import { Text } from '../../shared/text';
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
            <Sui className="h-10 w-7" />
            <div className={cl('flex flex-col', { 'mb-2': !!networkName })}>
                <SuiLogoTxt className="my-1" />
                {networkName && (
                    <div className="-mt-2 ml-0.5 whitespace-nowrap">
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
