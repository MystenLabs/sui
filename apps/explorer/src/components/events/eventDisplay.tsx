// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isMoveEvent,
    isNewObjectEvent,
    isTransferObjectEvent,
    isDeleteObjectEvent,
    isPublishEvent,
    isCoinBalanceChangeEvent,
} from '@mysten/sui.js';

import { isBigIntOrNumber } from '../../utils/numberUtil';
import { getOwnerStr } from '../../utils/objectUtils';
import { truncate } from '../../utils/stringUtils';

import type { Category } from '../../pages/transaction-result/TransactionResultType';
import type {
    MoveEvent,
    NewObjectEvent,
    ObjectId,
    SuiAddress,
    SuiEvent,
    TransferObjectEvent,
    DeleteObjectEvent,
    PublishEvent,
    CoinBalanceChangeEvent,
} from '@mysten/sui.js';
import type { LinkObj } from '~/ui/TableCard';

export type ContentItem = {
    label: string;
    value: string;
    monotypeClass: boolean;
    link?: boolean;
    category?: Category;
};

export type EventDisplayData = {
    top: {
        title: string;
        content: (ContentItem | ContentItem[])[];
    };
    fields?: {
        title: string;
        // css class name to apply to the 'Fields' sub-header
        titleStyle?: string;
        content: (ContentItem | ContentItem[])[];
    };
};

function addressContent(label: string, addr: SuiAddress) {
    return {
        label: label,
        value: addr,
        link: true,
        category: 'addresses' as Category,
        monotypeClass: true,
    };
}

function objectContent(label: string, id: ObjectId) {
    return {
        label: label,
        value: id,
        link: true,
        category: 'objects' as Category,
        monotypeClass: true,
    };
}

function fieldsContent(fields: { [key: string]: any } | undefined) {
    if (!fields) return [];
    return Object.keys(fields).map((k) => {
        return {
            label: k,
            value: fields[k].toString(),
            monotypeClass: true,
        };
    });
}

function contentLine(
    label: string,
    value: string,
    monotypeClass: boolean = false
) {
    return {
        label,
        value,
        monotypeClass,
    };
}

export function moveEventDisplay(event: MoveEvent): EventDisplayData {
    return {
        top: {
            title: 'Move Event',
            content: [
                contentLine('Type', event.type, true),
                addressContent('Sender', event.sender as string),
                contentLine('BCS', event.bcs, true),
            ],
        },
        fields: {
            title: 'Fields',
            titleStyle: 'itemfieldstitle',
            content: fieldsContent(event.fields),
        },
    };
}

export function newObjectEventDisplay(event: NewObjectEvent): EventDisplayData {
    const packMod = `${event.packageId}::${event.transactionModule}`;

    return {
        top: {
            title: 'New Object',
            content: [
                contentLine('Module', packMod, true),
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
    };
}

export function transferObjectEventDisplay(
    event: TransferObjectEvent
): EventDisplayData {
    return {
        top: {
            title: 'Transfer Object',
            content: [
                contentLine('Object Type', event.objectType, true),
                objectContent('Object ID', event.objectId),
                contentLine('Version', event.version.toString()),
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
    };
}

export function coinBalanceChangeEventDisplay(
    event: CoinBalanceChangeEvent
): EventDisplayData {
    return {
        top: {
            title: 'Coin Balance Change',
            content: [
                addressContent('Sender', event.sender),
                contentLine('Balance Change Type', event.changeType, true),
                contentLine('Coin Type', event.coinType),
                objectContent('Coin Object ID', event.coinObjectId),
                contentLine('Version', event.version.toString()),
                addressContent('Owner', getOwnerStr(event.owner)),
                contentLine('Amount', event.amount.toString()),
            ],
        },
    };
}

export function getAddressesLinks(item: ContentItem[]): LinkObj[] {
    return item.map((content) => {
        return {
            url: content.value,
            name: truncate(content.value, 20),
            copy: false,
            category: content.category,
            isLink: true,
        } as LinkObj;
    });
}

export function deleteObjectEventDisplay(
    event: DeleteObjectEvent
): EventDisplayData {
    const packMod = `${event.packageId}::${event.transactionModule}`;
    return {
        top: {
            title: 'Delete Object',
            content: [
                contentLine('Module', packMod, true),
                objectContent('Object ID', event.objectId),
                addressContent('Sender', event.sender),
            ],
        },
    };
}

export function publishEventDisplay(event: PublishEvent): EventDisplayData {
    return {
        top: {
            title: 'Publish',
            content: [
                addressContent('Sender', event.sender),
                contentLine('Package', event.packageId, true),
            ],
        },
    };
}

export function bigintDisplay(
    title: string,
    label: string,
    value: bigint
): EventDisplayData {
    return {
        top: {
            title: title,
            content: [contentLine(label, value.toString())],
        },
    };
}

export function eventToDisplay(event: SuiEvent) {
    if ('moveEvent' in event && isMoveEvent(event.moveEvent))
        return moveEventDisplay(event.moveEvent);

    if ('newObject' in event && isNewObjectEvent(event.newObject))
        return newObjectEventDisplay(event.newObject);

    if (
        'transferObject' in event &&
        isTransferObjectEvent(event.transferObject)
    )
        return transferObjectEventDisplay(event.transferObject);

    if ('deleteObject' in event && isDeleteObjectEvent(event.deleteObject))
        return deleteObjectEventDisplay(event.deleteObject);

    if (
        'coinBalanceChange' in event &&
        isCoinBalanceChangeEvent(event.coinBalanceChange)
    )
        return coinBalanceChangeEventDisplay(event.coinBalanceChange);

    if ('publish' in event && isPublishEvent(event.publish))
        return publishEventDisplay(event.publish);

    // TODO - once epoch and checkpoint pages exist, make these links
    if ('epochChange' in event && isBigIntOrNumber(event.epochChange))
        return bigintDisplay('Epoch Change', 'Epoch ID', event.epochChange);

    if ('checkpoint' in event && isBigIntOrNumber(event.checkpoint))
        return bigintDisplay('Checkpoint', 'Sequence #', event.checkpoint);

    return null;
}
