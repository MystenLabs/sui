// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import classnames from 'classnames/bind';

import { API_ENV } from '../../ApiProvider';
import { Text } from '../../shared/text';
import Icon, { SuiIcons } from '_components/icon';

import st from './Logo.module.scss';

const cl = classnames.bind(st);

const networkNames: Record<API_ENV, string> = {
    [API_ENV.local]: 'Local',
    [API_ENV.testNet]: 'Testnet',
    [API_ENV.devNet]: 'Devnet',
    [API_ENV.customRPC]: 'Custom RPC',
};

type LogoProps = {
    size?: 'normal' | 'big' | 'bigger' | 'huge';
    networkName?: API_ENV;
    className?: string;
};

const Logo = ({ size = 'normal', networkName, className }: LogoProps) => {
    return (
        <div
            className={cl(
                'inline-flex flex-nowrap items-center text-2xl',
                className
            )}
        >
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
