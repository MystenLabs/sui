// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiMoveNormalizedType } from '@mysten/sui.js';

/**
 * Converts a SuiMoveNormalizedType to string
 * @param param A parameter's normalized type of a function
 * @param functionTypeArgNames Parameters can be generic like 0x2::coin::Coin<T>.
 * T is provided on function level with the type_parameters field of SuiMoveNormalizedFunction that defines the abilities.
 * This parameter can be an array of strings that define the actual type or names like T1 that can be used to make the type of the parameter more specific. If
 * functionTypeArgNames or the index that the parameter expects are not defines then a default value T{index} is used.
 * @param str This function is recursive and this field is used to pass the already resolved type
 * @returns
 */
export function normalizedFunctionParameterTypeToString(
    param: SuiMoveNormalizedType,
    functionTypeArgNames?: string[],
    str = ''
): string {
    if (["Bool", "U8", "U16", "U32", "U64", "U128", "U256", "Address", "Signer"].indexOf(param.type) !== -1) {
        return str + param.content;
    }
    if ('TypeParameter' === param.type) {
        return (
            str +
            (functionTypeArgNames?.[param.content] ??
                `T${param.content}`)
        );
    }
    if ('Reference' === param.type || 'MutableReference' === param.type) {
        return normalizedFunctionParameterTypeToString(
            param.content,
            functionTypeArgNames,
            str
        );
    }
    if ('Vector' === param.type) {
        return (
            normalizedFunctionParameterTypeToString(
                param.content,
                functionTypeArgNames,
                `${str}Vector<`
            ) + '>'
        );
    }
    if ('Struct' === param.type) {
        const theType = param.content;
        const theTypeArgs = theType.typeArguments;
        const theTypeArgsStr = theTypeArgs
            .map((aTypeArg) =>
                normalizedFunctionParameterTypeToString(
                    aTypeArg,
                    functionTypeArgNames
                )
            )
            .join(', ');
        return `${[theType.address, theType.module, theType.name].join('::')}${
            theTypeArgsStr ? `<${theTypeArgsStr}>` : ''
        }`;
    }
    return str;
}

export function getNormalizedFunctionParameterTypeDetails(
    param: SuiMoveNormalizedType,
    functionTypeArgNames?: string[]
) {
    const paramTypeText = normalizedFunctionParameterTypeToString(
        param,
        functionTypeArgNames
    );
    return {
        isTxContext: paramTypeText === '0x2::tx_context::TxContext',
        paramTypeText,
    };
}
