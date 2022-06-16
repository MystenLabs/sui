import type { PermissionType } from './PermissionType';
import type { SuiAddress } from '@mysten/sui.js';

export interface Permission {
    id: string;
    origin: string;
    accounts: SuiAddress[];
    allowed: boolean | null;
    permissions: PermissionType[];
    createdDate: string;
    responseDate: string | null;
}
