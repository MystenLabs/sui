import { SuiObjectResponse, getObjectDisplay } from '@mysten/sui.js';

export const hasDisplayData = (obj: SuiObjectResponse) =>
    !!getObjectDisplay(obj).data;
