// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import { useCallback, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { PreviewTransfer } from './PreviewTransfer';
import { SendTokenForm } from './SendTokenForm';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import ActiveCoinsCard from '_components/active-coins-card';
import { SuiIcons } from '_components/icon';
import Overlay from '_components/overlay';

import type { SubmitProps } from './SendTokenForm';

function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [, setShowModal] = useState(true);

    const coinType = searchParams.get('type');
    const [showTransactionPreview, setShowTransactionPreview] =
        useState<boolean>(false);
    const [formData, setFormData] = useState<SubmitProps>();

    const navigate = useNavigate();

    const closeOverlay = useCallback(() => {
        navigate('/');
    }, [navigate]);

    const handleNextStep = useCallback((formData: SubmitProps) => {
        setShowTransactionPreview(true);
        setFormData(formData);
    }, []);

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    return (
        <Overlay
            showModal={true}
            setShowModal={setShowModal}
            title={showTransactionPreview ? 'Send Coins' : 'Review & Send'}
            closeOverlay={closeOverlay}
            closeIcon={SuiIcons.Close}
        >
            <div className="flex flex-col w-full mt-2.5">
                {showTransactionPreview && formData ? (
                    <BottomMenuLayout>
                        <Content>
                            <PreviewTransfer
                                coinType={coinType}
                                amount={formData.amount}
                                to={formData.to}
                                gasCostEstimation={formData.gasBudget}
                                approx={formData.isPayAllSui}
                            />
                        </Content>
                        <Menu
                            stuckClass="sendCoin-cta"
                            className="w-full px-0 pb-0 mx-0 gap-2.5"
                        >
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={() => setShowTransactionPreview(false)}
                                text={'Back'}
                                before={<ArrowLeft16 />}
                            />

                            <Button
                                type="button"
                                variant="primary"
                                size="tall"
                                text={'Send Now'}
                                after={<ArrowRight16 />}
                            />
                        </Menu>
                    </BottomMenuLayout>
                ) : (
                    <>
                        <div className="mb-7">
                            <ActiveCoinsCard activeCoinType={coinType} />
                        </div>

                        <SendTokenForm
                            onSubmit={handleNextStep}
                            coinType={coinType}
                            initialAmount={formData?.amount.toString() || ''}
                            initialTo={formData?.to || ''}
                        />
                    </>
                )}
            </div>
        </Overlay>
    );
}

export default TransferCoinPage;
