// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Body, Controller, Post, Route, Response, SuccessResponse } from 'tsoa';
import {
    AirdropService,
    AirdropClaimRequest,
    AirdropClaimResponse,
} from './airdropService';

interface ValidateErrorJSON {
    message: 'Validation failed';
    details: { [name: string]: unknown };
}

@Route('airdrop')
export class AirdropController extends Controller {
    @Response<ValidateErrorJSON>(422, 'Validation Failed', {
        message: 'Validation failed',
        details: {
            requestBody: {
                message: 'id is an excess property and therefore not allowed',
                value: '52907745-7672-470e-a803-a2f8feb52944',
            },
        },
    })
    @SuccessResponse('201', 'Created')
    @Post()
    public async claim(
        @Body() requestBody: AirdropClaimRequest
    ): Promise<AirdropClaimResponse> {
        this.setStatus(201);
        return await new AirdropService().claim(requestBody);
    }
}
