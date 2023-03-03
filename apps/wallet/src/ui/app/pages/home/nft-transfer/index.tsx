// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectId } from '@mysten/sui.js';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import { TransferNFTForm } from './TransferNFTForm';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import Overlay from '_components/overlay';
import { useGetObject, useOwnedNFT } from '_hooks';

function NftTransferPage() {
    const { nftId } = useParams();
    const address = useActiveAddress();

    // verify that the nft is owned by the user and is transferable
    const { data: objectData, isLoading } = useGetObject(nftId || '');
    const selectedNft = useOwnedNFT(objectData!, address);
    const objectId = selectedNft ? getObjectId(selectedNft.reference) : null;
    const navigate = useNavigate();

    return (
        <Overlay
            showModal={true}
            title="Send NFT"
            closeOverlay={() => navigate('/nfts')}
        >
            <div className="flex w-full flex-col h-full">
                <Loading loading={isLoading}>
                    {objectId ? (
                        <>
                            <div className="mb-7.5">
                                <NFTDisplayCard
                                    objectId={objectId}
                                    wideView
                                    size="sm"
                                />
                            </div>
                            <TransferNFTForm objectId={objectId} />
                        </>
                    ) : (
                        <Navigate to="/" replace />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}

export default NftTransferPage;
