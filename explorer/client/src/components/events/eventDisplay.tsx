import {
    isMoveEvent,
    isNewObjectEvent,
    isTransferObjectEvent,
    isDeleteObjectEvent,
    isPublishEvent,
} from '@mysten/sui.js';

import { isBigIntOrNumber } from '../../utils/numberUtil';
import { getOwnerStr } from '../../utils/objectUtils';

import type {
    MoveEvent,
    NewObjectEvent,
    ObjectId,
    SuiAddress,
    SuiEvent,
    TransferObjectEvent,
    DeleteObjectEvent,
    PublishEvent,
} from '@mysten/sui.js';

export function moveEventDisplay(event: MoveEvent) {
    return {
        top: {
            title: 'Move Event',
            content: [
                {
                    label: 'Type',
                    value: event.type,
                    monotypeClass: true,
                },
                addressContent('Sender', event.sender as string),
                {
                    label: 'BCS',
                    value: event.bcs,
                    monotypeClass: true,
                },
            ],
        },
        fields: {
            title: 'Fields',
            titleStyle: 'itemfieldstitle',
            content: fieldsContent(event.fields),
        },
    };
}

function addressContent(label: string, addr: SuiAddress) {
    return {
        label: label,
        value: addr,
        link: true,
        category: 'addresses',
        monotypeClass: true,
    };
}

function objectContent(label: string, id: ObjectId) {
    return {
        label: label,
        value: id,
        link: true,
        category: 'objects',
        monotypeClass: true,
    };
}

function fieldsContent(fields: { [key: string]: any }) {
    return Object.keys(fields).map((k) => {
        return {
            label: k,
            value: fields[k].toString(),
            monotypeClass: true,
        };
    });
}

export function newObjectEventDisplay(event: NewObjectEvent) {
    return {
        top: {
            title: 'New Object',
            content: [
                {
                    label: 'Module',
                    value: `${event.packageId}::${event.transactionModule}`,
                    monotypeClass: true,
                },
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
        fields: null,
    };
}

export function transferObjectEventDisplay(event: TransferObjectEvent) {
    return {
        top: {
            title: 'Transfer Object',
            content: [
                {
                    label: 'Type',
                    value: event.type,
                    monotypeClass: true,
                },
                objectContent('Object ID', event.objectId),
                {
                    label: 'Version',
                    value: event.version.toString(),
                    monotypeClass: false,
                },
                [
                    addressContent('', event.sender),
                    addressContent('', getOwnerStr(event.recipient)),
                ],
            ],
        },
        fields: null,
    };
}

export function deleteObjectEventDisplay(event: DeleteObjectEvent) {
    return {
        top: {
            title: 'Delete Object',
            content: [
                {
                    label: 'Module',
                    value: `${event.packageId}::${event.transactionModule}`,
                    monotypeClass: true,
                },
                objectContent('Object ID', event.objectId),
                addressContent('Sender', event.sender),
            ],
        },
        fields: null,
    };
}

export function publishEventDisplay(event: PublishEvent) {
    return {
        top: {
            title: 'Publish',
            content: [
                addressContent('Sender', event.sender),
                objectContent('Package', event.packageId),
            ],
        },
        fields: null,
    };
}

export function bigintDisplay(title: string, label: string, value: bigint) {
    return {
        top: {
            title: title,
            content: [
                {
                    label: label,
                    value: value.toString(),
                    monotypeClass: false,
                },
            ],
        },
        fields: null,
    };
}

export function eventToDisplay(event: SuiEvent) {
    console.log('event to display', event);

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

    if ('publish' in event && isPublishEvent(event.publish))
        return publishEventDisplay(event.publish);

    // TODO - once epoch and checkpoint pages exist, make these links
    if ('epochChange' in event && isBigIntOrNumber(event.epochChange))
        return bigintDisplay('Epoch Change', 'Epoch ID', event.epochChange);

    if ('checkpoint' in event && isBigIntOrNumber(event.checkpoint))
        return bigintDisplay('Checkpoint', 'Sequence #', event.checkpoint);

    return null;
}
