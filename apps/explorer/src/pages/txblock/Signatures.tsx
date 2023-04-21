// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    toB64,
    fromSerializedSignature,
    getGasData,
    getTransactionSender,
    getTransactionSignature,
    normalizeSuiAddress,
    type SuiAddress,
    type SuiTransactionBlockResponse,
    type SignaturePubkeyPair,
} from '@mysten/sui.js';

import { DescriptionItem, DescriptionList } from '~/ui/DescriptionList';
import { AddressLink } from '~/ui/InternalLink';
import { Tab, TabGroup, TabList } from '~/ui/Tabs';
import { Text } from '~/ui/Text';

function SignaturePanel({
    title,
    signature,
}: {
    title: string;
    signature: SignaturePubkeyPair;
}) {
    return (
        <TabGroup>
            <TabList>
                <Tab>{title}</Tab>
            </TabList>
            <DescriptionList>
                <DescriptionItem title="Scheme">
                    <Text variant="pBody/medium" color="steel-darker">
                        {signature.signatureScheme}
                    </Text>
                </DescriptionItem>
                <DescriptionItem title="Address">
                    <AddressLink
                        noTruncate
                        address={signature.pubKey.toSuiAddress()}
                    />
                </DescriptionItem>
                <DescriptionItem title="Signature">
                    <Text variant="pBody/medium" color="steel-darker">
                        {toB64(signature.signature)}
                    </Text>
                </DescriptionItem>
            </DescriptionList>
        </TabGroup>
    );
}

function getSignatureFromAddress(
    signatures: SignaturePubkeyPair[],
    suiAddress: SuiAddress
) {
    return signatures.find(
        (signature) =>
            signature.pubKey.toSuiAddress() === normalizeSuiAddress(suiAddress)
    );
}

interface Props {
    transaction: SuiTransactionBlockResponse;
}

export function Signatures({ transaction }: Props) {
    const sender = getTransactionSender(transaction);
    const gasData = getGasData(transaction);
    const transactionSignatures = getTransactionSignature(transaction);

    if (!transactionSignatures) return null;

    const isSponsoredTransaction = gasData?.owner !== sender;

    const deserializedTransactionSignatures = transactionSignatures.map(
        (signature) => fromSerializedSignature(signature)
    );

    const userSignature = getSignatureFromAddress(
        deserializedTransactionSignatures,
        sender!
    );

    const sponsorSignature = isSponsoredTransaction
        ? getSignatureFromAddress(
              deserializedTransactionSignatures,
              gasData!.owner
          )
        : null;

    return (
        <div className="flex flex-col gap-8">
            {userSignature && (
                <SignaturePanel
                    title="User Signature"
                    signature={userSignature}
                />
            )}
            {sponsorSignature && (
                <SignaturePanel
                    title="Sponsor Signature"
                    signature={sponsorSignature}
                />
            )}
        </div>
    );
}
