// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    gatewayServiceAPI,
    CallRequest,
    ObjectInfoResponse,
} from './gatewayServiceAPI';

export interface TransactionResponse {
    gasUsed: number;
    objectEffectsSummary: ObjectEffectsSummary;
}

export interface ObjectEffectsSummary {
    created_objects: ObjectEffect[];
    mutated_objects: ObjectEffect[];
    unwrapped_objects: ObjectEffect[];
    deleted_objects: ObjectEffect[];
    wrapped_objects: ObjectEffect[];
    events: EventEffect[];
}

export interface ObjectEffect {
    type: string;
    id: string;
    version: string;
    object_digest: string;
}

export interface EventEffect {
    type: string;
    contents: string;
}

/**
 * A connection to a Sui Gateway endpoint
 */
export class Connection {
    /** @internal */ _endpointURL: string;
    /** @internal */ _gatewayAPI;

    /**
     * Establish a connection to a Sui Gateway endpoint
     *
     * @param endpoint URL to the Sui Gateway endpoint
     */
    constructor(endpoint: string) {
        this._endpointURL = endpoint;
        this._gatewayAPI = gatewayServiceAPI({ baseUrl: endpoint });
    }

    /**
     * Retrieve all managed addresses for this client.
     */
    public async getAddresses(): Promise<string[]> {
        const {
            data: { addresses },
        } = await this._gatewayAPI.getAddresses({});
        return addresses;
    }

    /**
     * Execute a Move call transaction by calling the specified function in the
     * module of the given package
     */
    public async callMoveFunction(
        request: CallRequest
    ): Promise<TransactionResponse> {
        const {
            data: { gasUsed, objectEffectsSummary },
        } = await this._gatewayAPI.callMoveFunction(request);
        return {
            gasUsed,
            objectEffectsSummary: objectEffectsSummary as ObjectEffectsSummary,
        };
    }

    /**
     * Returns list of object ids owned by an address.
     */
    public async getObjectIds(address: string): Promise<string[]> {
        const {
            data: { objects },
        } = await this._gatewayAPI.getObjects({ address });
        return objects.map(({ objectId }) => objectId);
    }

    /**
     * Returns the object information for a specified object.
     */
    public async getObjectInfo(
        objectId: string
    ): Promise<ObjectInfoResponse | null> {
        try {
            const { data } = await this._gatewayAPI.getObjectInfo({ objectId });
            return data;
        } catch (error) {
            console.error('Encounter error for ', objectId, error);
        }
        return null;
    }

    /**
     * Returns all objects owned by an address of a given type(optional)
     */
    public async bulkFetchObjects(
        address: string,
        objectType?: string
    ): Promise<ObjectInfoResponse[]> {
        // TODO<https://github.com/MystenLabs/sui/issues/803>: support get objects by types in Gateway
        const objectIds = await this.getObjectIds(address);
        // TODO<https://github.com/MystenLabs/sui/issues/828> Gateway needs to support
        // concurrent requests before we can use the following code
        // const objects = await Promise.all(
        //     objectIds.map(async (id) => await this.getObjectInfo(id))
        // );
        const objects = [];
        for (const id of objectIds) {
            const info = await this.getObjectInfo(id);
            if (info != null) {
                objects.push(info);
            }
        }
        return objects.filter(
            (object) => objectType == null || object.objType === objectType
        );
    }

    /**
     * Returns all object ids owned by an address of a given type(optional)
     */
    public async bulkFetchObjectIds(
        address: string,
        objectType?: string
    ): Promise<string[]> {
        // TODO<https://github.com/MystenLabs/sui/issues/803>: support get objects by types in Gateway
        const objects = await this.bulkFetchObjects(address, objectType);
        return objects.map((object) => object.id);
    }
}

export { CallRequest, ObjectInfoResponse };
