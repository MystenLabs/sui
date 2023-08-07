// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { hasPublicTransfer, formatAddress } from '@mysten/sui.js';
import cl from 'classnames';
import { Navigate, useSearchParams } from 'react-router-dom';

import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { Button } from '_app/shared/ButtonUI';
import { Link } from '_app/shared/Link';
import { Collapse } from '_app/shared/collapse';
import { LabelValueItem } from '_components/LabelValueItem';
import { LabelValuesContainer } from '_components/LabelValuesContainer';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import { useGetNFTMeta, useNFTBasicData, useOwnedNFT } from '_hooks';
import { useExplorerLink } from '_src/ui/app/hooks/useExplorerLink';
import PageTitle from '_src/ui/app/shared/PageTitle';

function NFTDetailsPage() {
    const [searchParams] = useSearchParams();
    const nftId = searchParams.get('objectId');
    const accountAddress = useActiveAddress();
    const { data: objectData, isLoading } = useOwnedNFT(
        nftId || '',
        accountAddress
    );
    const isTransferable = !!objectData && hasPublicTransfer(objectData);
    const { nftFields, fileExtensionType, filePath } =
        useNFTBasicData(objectData);
    // Extract either the attributes, or use the top-level NFT fields:
    const metaFields =
        nftFields?.metadata?.fields?.attributes?.fields ||
        Object.entries(nftFields ?? {})
            .filter(([key]) => key !== 'id')
            .reduce(
                (acc, [key, value]) => {
                    acc.keys.push(key);
                    acc.values.push(value);
                    return acc;
                },
                { keys: [] as string[], values: [] as string[] }
            );
    const metaKeys: string[] = metaFields ? metaFields.keys : [];
    const metaValues = metaFields ? metaFields.values : [];
    const { data: nftDisplayData, isLoading: isLoadingDisplay } = useGetNFTMeta(
        nftId || ''
    );
    const objectExplorerLink = useExplorerLink({
        type: ExplorerLinkType.object,
        objectID: nftId || '',
    });
    const ownerAddress =
        (typeof objectData?.owner === 'object' &&
            'AddressOwner' in objectData.owner &&
            objectData.owner.AddressOwner) ||
        '';
    const ownerExplorerLink = useExplorerLink({
        type: ExplorerLinkType.address,
        address: ownerAddress,
    });
    return (
        <div
            className={cl('flex flex-1 flex-col flex-nowrap gap-5', {
                'items-center': isLoading || isLoadingDisplay,
            })}
        >
            <Loading loading={isLoading || isLoadingDisplay}>
                {objectData ? (
                    <>
                        <PageTitle back="/nfts" />
                        <div className="flex flex-1 flex-col flex-nowrap items-stretch gap-8">
                            <div className="flex flex-col flex-nowrap items-center gap-3 self-center">
                                <NFTDisplayCard
                                    objectId={nftId!}
                                    size="lg"
                                    borderRadius="xl"
                                />
                                {nftId ? (
                                    <Link
                                        color="steelDark"
                                        weight="semibold"
                                        size="captionSmall"
                                        href={objectExplorerLink || ''}
                                        text="VIEW ON EXPLORER"
                                        after={
                                            <ArrowRight16 className="-rotate-45" />
                                        }
                                    />
                                ) : null}
                            </div>
                            <LabelValuesContainer>
                                {ownerExplorerLink ? (
                                    <LabelValueItem
                                        label="Owner"
                                        value={
                                            <Link
                                                color="suiDark"
                                                weight="medium"
                                                size="body"
                                                mono
                                                href={ownerExplorerLink}
                                                text={formatAddress(
                                                    ownerAddress
                                                )}
                                                title="View on Sui Explorer"
                                            />
                                        }
                                    />
                                ) : null}
                                <LabelValueItem
                                    label="Object Id"
                                    value={
                                        nftId ? (
                                            <Link
                                                color="suiDark"
                                                weight="medium"
                                                size="body"
                                                mono
                                                href={objectExplorerLink || ''}
                                                text={formatAddress(nftId)}
                                                title="View on Sui Explorer"
                                            />
                                        ) : null
                                    }
                                />
                                <LabelValueItem
                                    label="Media Type"
                                    value={
                                        filePath &&
                                        fileExtensionType.name &&
                                        fileExtensionType.type
                                            ? `${fileExtensionType.name} ${fileExtensionType.type}`
                                            : '-'
                                    }
                                />
                            </LabelValuesContainer>
                            <Collapse title="Details" initialIsOpen>
                                <LabelValuesContainer>
                                    <LabelValueItem
                                        label="Name"
                                        value={nftDisplayData?.name}
                                    />
                                    <LabelValueItem
                                        label="Description"
                                        value={nftDisplayData?.description}
                                        multiline
                                    />
                                    <LabelValueItem
                                        label="Creator"
                                        value={nftDisplayData?.creator}
                                        parseUrl
                                    />
                                    <LabelValueItem
                                        label="Link"
                                        value={nftDisplayData?.link}
                                        parseUrl
                                    />
                                    <LabelValueItem
                                        label="Website"
                                        value={nftDisplayData?.projectUrl}
                                        parseUrl
                                    />
                                </LabelValuesContainer>
                            </Collapse>
                            {metaKeys.length ? (
                                <Collapse title="Attributes" initialIsOpen>
                                    <LabelValuesContainer>
                                        {metaKeys.map((aKey, idx) => (
                                            <LabelValueItem
                                                key={aKey}
                                                label={aKey}
                                                value={
                                                    typeof metaValues[idx] ===
                                                    'object'
                                                        ? JSON.stringify(
                                                              metaValues[idx]
                                                          )
                                                        : metaValues[idx]
                                                }
                                            />
                                        ))}
                                    </LabelValuesContainer>
                                </Collapse>
                            ) : null}
                            <div className="mb-3 flex flex-1 items-end">
                                <Button
                                    variant="primary"
                                    size="tall"
                                    disabled={!isTransferable}
                                    to={`/nft-transfer/${nftId}`}
                                    title={
                                        isTransferable
                                            ? undefined
                                            : "Unable to send. NFT doesn't have public transfer method"
                                    }
                                    text="Send NFT"
                                    after={<ArrowRight16 />}
                                />
                            </div>
                        </div>
                    </>
                ) : (
                    <Navigate to="/nfts" replace={true} />
                )}
            </Loading>
        </div>
    );
}

export default NFTDetailsPage;
