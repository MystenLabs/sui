// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, CheckFill16 } from '@mysten/icons';

import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Loading from '_components/loading';
import Logo from '_components/logo';
import { useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

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

                                    <div className="mt-6 flex gap-2 flex-col">
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

                            <div className="flex sticky pb-10 m-auto w-[300px] -bottom-px bg-sui-lightest">
                                <Button
                                    to="/initialize/select"
                                    size="tall"
                                    text="Get Started"
                                    after={<ArrowRight16 />}
                                />
                            </div>
                        </BottomMenuLayout>
                    </div>
                </div>
            </Loading>
        </PageLayout>
    );
};

export default WelcomePage;
