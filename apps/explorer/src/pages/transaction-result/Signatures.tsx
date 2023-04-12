// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    toB64,
    fromSerializedSignature,
    normalizeSuiAddress,
    type SuiAddress,
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
                    <Text variant="p1/medium" color="steel-darker">
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
                    <Text variant="p1/medium" color="steel-darker">
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
    sender: string;
    gasOwner: string;
    signatures: string[];
}

export function Signatures({ sender, gasOwner, signatures }: Props) {
    const isSponsoredTransaction = gasOwner !== sender;
    const deserializedTransactionSignatures = signatures.map((signature) =>
        fromSerializedSignature(signature)
    );
    const userSignature = getSignatureFromAddress(
        deserializedTransactionSignatures,
        sender
    );
    const sponsorSignature = isSponsoredTransaction
        ? getSignatureFromAddress(deserializedTransactionSignatures, gasOwner)
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
