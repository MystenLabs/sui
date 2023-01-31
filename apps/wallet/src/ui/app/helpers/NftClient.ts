// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SuiObject, is } from '@mysten/sui.js';

import type {
    GetObjectDataResponse,
    JsonRpcProvider,
    SuiMoveObject,
} from '@mysten/sui.js';

export interface WithIds {
    objectIds: string[];
}

type FetchFnParser<RpcResponse, DataModel> = (
    typedData: RpcResponse,
    suiObject: SuiObject,
    rpcResponse: GetObjectDataResponse
) => DataModel | undefined;

type SuiObjectParser<RpcResponse, DataModel> = {
    parser: FetchFnParser<RpcResponse, DataModel>;
    regex: RegExp;
};

type ID = {
    id: string;
};

type Bag = {
    type: string;
    fields: {
        id: ID;
        size: number;
    };
};

type NftRpcResponse = {
    logical_owner: string;
    bag?: Bag;
};

type NftRaw = {
    id: string;
    logicalOwner: string;
    bagId?: string;
};

type DomainRpcBase<T> = {
    id: ID;
    name: {
        type: string;
        fields: {
            dummy_field: boolean;
        };
    };
    value: {
        type: string;
        fields: T;
    };
};

type UrlDomain = {
    url: string;
};

type UrlDomainRpcResponse = DomainRpcBase<UrlDomain>;

type DisplayDomain = {
    description: string;
    name: string;
};
type DisplayDomainRpcResponse = DomainRpcBase<DisplayDomain>;

type NftDomains = {
    url: string;
    name: string;
    description: string;
    attributeKeys: string[];
    attributeValues: string[];
};

export type AttributionDomain = {
    map: {
        type: string;
        fields: {
            contents: {
                type: string;
                fields: {
                    key: string;
                    value: string;
                };
            }[];
        };
    };
};

export type AttributionDomainBagRpcResponse = DomainRpcBase<AttributionDomain>;

export type Nft = {
    nft: NftRaw;
    fields?: Partial<NftDomains>;
};

const NftRegex =
    /(0x[a-f0-9]{39,40})::nft::Nft<0x[a-f0-9]{39,40}::([a-zA-Z]{1,})::([a-zA-Z]{1,})>/;
const UrlDomainBagRegex =
    /0x2::dynamic_field::Field<(0x[a-f0-9]{39,40})::utils::Marker<(0x[a-f0-9]{39,40})::display::UrlDomain>, (0x[a-f0-9]{39,40})::display::UrlDomain>/;
const DisplayDomainBagRegex =
    /0x2::dynamic_field::Field<(0x[a-f0-9]{39,40})::utils::Marker<(0x[a-f0-9]{39,40})::display::DisplayDomain>, (0x[a-f0-9]{39,40})::display::DisplayDomain>/;
const AttributesDomainBagRegex =
    /dynamic_field::Field<(0x[a-f0-9]{39,40})::utils::Marker<(0x[a-f0-9]{39,40})::display::AttributesDomain>, (0x[a-f0-9]{39,40})::display::AttributesDomain>/;

export const NftParser: SuiObjectParser<NftRpcResponse, NftRaw> = {
    parser: (data, suiData, rpcResponse) => {
        if (
            typeof rpcResponse.details === 'object' &&
            'data' in rpcResponse.details
        ) {
            const { owner } = rpcResponse.details;

            const matches = (suiData.data as SuiMoveObject).type.match(
                NftRegex
            );
            if (!matches) {
                return undefined;
            }
            const packageObjectId = matches[1];
            const packageModule = matches[2];
            const packageModuleClassName = matches[3];

            return {
                owner,
                type: suiData.data.dataType,
                id: rpcResponse.details.reference.objectId,
                packageObjectId,
                packageModule,
                packageModuleClassName,
                rawResponse: rpcResponse,
                logicalOwner: data.logical_owner,
                bagId: data.bag?.fields.id.id,
            };
        }
        return undefined;
    },
    regex: NftRegex,
};

const isObjectExists = (o: GetObjectDataResponse) => o.status === 'Exists';

const isTypeMatchRegex = (d: GetObjectDataResponse, regex: RegExp) => {
    const { details } = d;
    if (is(details, SuiObject)) {
        const { data } = details;
        if ('type' in data) {
            return data.type.match(regex);
        }
    }
    return false;
};

export const parseBagDomains = (domains: GetObjectDataResponse[]) => {
    const response: Partial<NftDomains> = {};
    const urlDomain = domains.find((d) =>
        isTypeMatchRegex(d, UrlDomainBagRegex)
    );
    const displayDomain = domains.find((d) =>
        isTypeMatchRegex(d, DisplayDomainBagRegex)
    );
    const attributesDomain = domains.find((d) =>
        isTypeMatchRegex(d, AttributesDomainBagRegex)
    );

    if (
        urlDomain &&
        is(urlDomain.details, SuiObject) &&
        'fields' in urlDomain.details.data
    ) {
        const { data } = urlDomain.details;
        response.url = (data.fields as UrlDomainRpcResponse).value.fields.url;
    }
    if (
        displayDomain &&
        is(displayDomain.details, SuiObject) &&
        'fields' in displayDomain.details.data
    ) {
        const { data } = displayDomain.details;
        response.description = (
            data.fields as DisplayDomainRpcResponse
        ).value.fields.description;
        response.name = (
            data.fields as DisplayDomainRpcResponse
        ).value.fields.name;

        if (
            attributesDomain &&
            is(attributesDomain.details, SuiObject) &&
            'fields' in attributesDomain.details.data
        ) {
            const { data } = attributesDomain.details;
            response.attributeKeys = (
                data.fields as AttributionDomainBagRpcResponse
            ).value.fields.map.fields.contents.map(
                (attribute) => attribute.fields.key
            );

            response.attributeValues = (
                data.fields as AttributionDomainBagRpcResponse
            ).value.fields.map.fields.contents.map(
                (attribute) => attribute.fields.value
            );
        }
    }

    return response;
};

const UrlDomainRegex = /(0x[a-f0-9]{39,40})::display::UrlDomain/;
const DisplayDomainRegex = /(0x[a-f0-9]{39,40})::display::DisplayDomain/;
const AttributesDomainRegex = /(0x[a-f0-9]{39,40})::display::AttributesDomain/;

export const parseDynamicDomains = (domains: GetObjectDataResponse[]) => {
    const response: Partial<NftDomains> = {};

    const urlDomain = domains.find((d) => isTypeMatchRegex(d, UrlDomainRegex));

    const displayDomain = domains.find((d) =>
        isTypeMatchRegex(d, DisplayDomainRegex)
    );
    const attibutesDomain = domains.find((d) =>
        isTypeMatchRegex(d, AttributesDomainRegex)
    );

    if (
        urlDomain &&
        is(urlDomain.details, SuiObject) &&
        'fields' in urlDomain.details.data
    ) {
        const { data } = urlDomain.details;
        response.url = (data.fields as UrlDomain).url;
    }
    if (
        displayDomain &&
        is(displayDomain.details, SuiObject) &&
        'fields' in displayDomain.details.data
    ) {
        const { data } = displayDomain.details;
        response.description = (data.fields as DisplayDomain).description;
        response.name = (data.fields as DisplayDomain).name;
    }

    if (
        attibutesDomain &&
        is(attibutesDomain.details, SuiObject) &&
        'fields' in attibutesDomain.details.data
    ) {
        const { data } = attibutesDomain.details;
        response.attributeKeys = (
            data.fields as AttributionDomain
        ).map.fields.contents.map((attribute) => attribute.fields.key);

        response.attributeValues = (
            data.fields as AttributionDomain
        ).map.fields.contents.map((attribute) => attribute.fields.value);
    }

    return response;
};

export class NftClient {
    private provider: JsonRpcProvider;

    constructor(provider: JsonRpcProvider) {
        this.provider = provider;
    }

    parseObjects = async (
        objects: GetObjectDataResponse[]
    ): Promise<NftRaw[]> => {
        const parsedObjects = objects
            .filter(isObjectExists)
            .map((object) => {
                if (
                    is(object.details, SuiObject) &&
                    'type' in object.details.data &&
                    object.details.data.type.match(NftParser.regex)
                ) {
                    return NftParser.parser(
                        object.details.data.fields as NftRpcResponse,
                        object.details,
                        object
                    );
                }
                return undefined;
            })
            .filter((object): object is NftRaw => !!object);

        return parsedObjects;
    };

    fetchAndParseObjectsById = async (ids: string[]): Promise<NftRaw[]> => {
        if (ids.length === 0) {
            return new Array<NftRaw>();
        }
        const objects = await this.provider.getObjectBatch(ids);
        return this.parseObjects(objects);
    };

    getBagContent = async (bagId: string) => {
        const bagObjects = await this.provider.getObjectsOwnedByObject(bagId);
        const objectIds = bagObjects.map((bagObject) => bagObject.objectId);
        return this.provider.getObjectBatch(objectIds);
    };

    getDynamicFields = async (parentdId: string) => {
        const objects = await this.provider.getDynamicFields(parentdId);
        const objectIds = objects.data.map((_) => _.objectId);
        return this.provider.getObjectBatch(objectIds);
    };

    getNftsById = async (params: WithIds): Promise<Nft[]> => {
        const nfts = await this.fetchAndParseObjectsById(params.objectIds);

        const bags = await Promise.all(
            nfts.map(async (nft) => {
                const content = nft.bagId
                    ? await this.getBagContent(nft.bagId)
                    : await this.getDynamicFields(nft.id);

                return {
                    nftId: nft.id,
                    content: nft.bagId
                        ? parseBagDomains(content)
                        : parseDynamicDomains(content),
                };
            })
        );
        const bagsByNftId = new Map(bags.map((b) => [b.nftId, b]));

        return nfts.map((nft) => {
            const fields = bagsByNftId.get(nft.id);
            return {
                nft,
                fields: fields?.content,
            };
        });
    };
}
