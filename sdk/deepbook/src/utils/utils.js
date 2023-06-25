"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getPoolInfoByRecords = void 0;
function getPoolInfoByRecords(tokenType1, tokenType2, records) {
    for (const ele of records.pools) {
        if (ele.type.indexOf(tokenType1) != -1 &&
            ele.type.indexOf(tokenType2) != -1 &&
            ele.type.indexOf(tokenType1) < ele.type.indexOf(tokenType2)) {
            return {
                needChange: false,
                clob_v2: String(ele.clob_v2),
                type: String(ele.type),
                tickSize: ele.tickSize,
            };
        }
        else if (ele.type.indexOf(tokenType1) != -1 &&
            ele.type.indexOf(tokenType2) != -1 &&
            ele.type.indexOf(tokenType1) > ele.type.indexOf(tokenType2)) {
            return {
                needChange: true,
                clob_v2: String(ele.clob_v2),
                type: String(ele.type),
                tickSize: ele.tickSize,
            };
        }
    }
    throw new Error('Pool not found');
}
exports.getPoolInfoByRecords = getPoolInfoByRecords;
