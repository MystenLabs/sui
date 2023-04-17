// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CheckFill16 } from '@mysten/icons';
import cl from 'classnames';
import { Link } from 'react-router-dom';

import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

import st from './Welcome.module.scss';

const VALUE_PROP = [
    'Send, receive tokens and NFTs',
    'Stake SUI to earn rewards. Help the Sui network remain decentralized.',
    'Explore apps on Sui blockchain',
    'Quickly revoke access connection given to apps',
    'Track your Sui network activity',
];

const WelcomePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    return (
        <PageLayout forceFullscreen={true}>
            <Loading loading={checkingInitialized}>
                <div className="flex flex-col flex-nowrap items-center justify-center">
                    <div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col flex-nowrap items-center justify-center w-popup-width h-popup-height">
                        <BottomMenuLayout>
                            <Content className="flex flex-col flex-nowrap items-center p-7.5 pb-0">
                                <div className="mt-7.5 text-hero">
                                    <Logo />
                                </div>

                                <div className="mx-auto mt-7">
                                    <div className="text-center">
                                        <Heading
                                            variant="heading2"
                                            color="gray-90"
                                            as="h1"
                                            weight="bold"
                                        >
                                            Welcome to Sui Wallet
                                        </Heading>
                                        <div className="mt-3">
                                            <Text
                                                variant="pBody"
                                                color="steel-dark"
                                                weight="medium"
                                            >
                                                Connecting you to the
                                                decentralized web and Sui
                                                network.
                                            </Text>
                                        </div>
                                    </div>

                                    <div className="mt-5 flex gap-2 flex-col">
                                        {VALUE_PROP.map((value) => (
                                            <div
                                                key={value}
                                                className="flex gap-2 items-center border bg-sui-light/40 border-sui/30 border-solid rounded-xl px-3 py-2"
                                            >
                                                <CheckFill16 className="text-steel flex-none w-4 h-4" />

                                                <Text
                                                    variant="pBody"
                                                    color="steel-darker"
                                                    weight="medium"
                                                >
                                                    {value}
                                                </Text>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            </Content>
                            <div className={st.getStarted}>
                                <Link
                                    to="/initialize/select"
                                    className={cl(st.cta, 'btn', 'primary')}
                                >
                                    Get Started
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className={cl(st.arrowLeft)}
                                    />
                                </Link>
                            </div>
                        </BottomMenuLayout>
                    </div>
                </div>
            </Loading>
        </PageLayout>
    );
};

export default WelcomePage;
