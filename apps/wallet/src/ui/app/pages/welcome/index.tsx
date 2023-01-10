// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

const WelcomePage = () => {
    const checkingInitialized = useInitializedGuard(false);
    return (
        <PageLayout forceFullscreen={true}>
            <Loading loading={checkingInitialized}>
                <div className="flex flex-col flex-nowrap items-center justify-center">
                    <div className="pt-6 rounded-20 bg-alice-blue shadow-wallet-content flex flex-col flex-nowrap items-center justify-center w-popup-width h-popup-height">
                        <BottomMenuLayout>
                            <Content className="flex flex-col flex-nowrap items-center p-7.5 pb-0">
                                <Logo
                                    size="normal"
                                    className="text-hero mt-7.5"
                                    txt={true}
                                />
                                <div className="mx-auto text-center mt-12">
                                    <Heading
                                        variant="heading2"
                                        color="gray-90"
                                        as="h1"
                                        weight="bold"
                                    >
                                        Welcome to Sui Wallet
                                    </Heading>
                                    <div className="mt-5">
                                        <Text
                                            variant="p1"
                                            color="steel-dark"
                                            weight="medium"
                                        >
                                            Connecting you to the decentralized
                                            web and Sui network.
                                        </Text>
                                    </div>
                                    <div className="flex gap-2 mt-10 items-center">
                                        <Icon
                                            icon={SuiIcons.Checkmark}
                                            className="text-success text-[8px]"
                                        />

                                        <Text
                                            variant="body"
                                            color="steel-dark"
                                            weight="medium"
                                        >
                                            Buy, store, send and swap tokens
                                        </Text>
                                    </div>
                                    <div className="flex gap-2 mt-3 items-center">
                                        <Icon
                                            icon={SuiIcons.Checkmark}
                                            className="text-success text-[8px]"
                                        />
                                        <Text
                                            variant="body"
                                            color="steel-dark"
                                            weight="medium"
                                        >
                                            Explore blockchain apps
                                        </Text>
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
