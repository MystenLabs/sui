// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiMoveNormalizedType } from '@mysten/sui.js';

type TypeReference =
    | {
          address: string;
          module: string;
          name: string;
          typeArguments?: SuiMoveNormalizedType[];
      }
    | string
    | number;

// Get content inside <> and split by , to get underlying object types
function getContentInsideBrackets(input: string) {
    return input?.slice(input?.indexOf('<') + 1, input?.lastIndexOf('>'));
}

function splitByCommaExcludingBrackets(input: string) {
    const regex = /,(?![^<>]*>)/;
    return input.split(regex).map((part) => part.trim());
}

export function extractSerializationType(
    type: SuiMoveNormalizedType
): TypeReference {
    if (typeof type === 'string') {
        return type;
    }

    if ('TypeParameter' in type) {
        return type.TypeParameter;
    }

    if ('Reference' in type) {
        return extractSerializationType(type.Reference);
    }

    if ('MutableReference' in type) {
        return extractSerializationType(type.MutableReference);
    }

    if ('Vector' in type) {
        return extractSerializationType(type.Vector);
    }

    if ('Struct' in type) {
        return type.Struct;
    }

    return type;
}

export function getFieldTypeValue(
    type: SuiMoveNormalizedType,
    objectType: string
) {
    const normalizedType = extractSerializationType(type);
    if (typeof normalizedType === 'string') {
        return {
            displayName: normalizedType,
            normalizedType: normalizedType,
        };
    }
    // For TypeParameter index return the type string index after splitting, where the third index is the type
    if (typeof normalizedType === 'number') {
        const typeParameter = splitByCommaExcludingBrackets(
            getContentInsideBrackets(objectType)
        );

        return {
            displayName:
                typeParameter?.[normalizedType]?.split('::').pop() || '',
            normalizedType: normalizedType,
        };
    }

    // For nested Structs type.typeArguments  append the typeArguments to the name
    // Balance<XUS> || Balance<LSP<SUI, USDT>>
    const { address, module, name } = normalizedType;
    let typeParam = '';

    if (normalizedType.typeArguments?.length) {
        typeParam = `<${normalizedType.typeArguments
            .map(
                (typeArg) => getFieldTypeValue(typeArg, objectType).displayName
            )
            .join(', ')}>`;
    }

    return {
        displayName: `${normalizedType.name}${typeParam}`,
        normalizedType: `${address}::${module}::${name}`,
    };
}
