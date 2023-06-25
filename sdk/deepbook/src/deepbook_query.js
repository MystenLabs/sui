"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.DeepBook_query = void 0;
const sui_js_1 = require("@mysten/sui.js");
class DeepBook_query {
    constructor(provider = new sui_js_1.JsonRpcProvider(sui_js_1.testnetConnection), currentAddress) {
        this.provider = provider;
        this.currentAddress = currentAddress;
    }
    /**
     * @description get the order status
     * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
     * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
     * @param poolId: the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
     * @param orderId the order id, eg: "1"
     * @param accountCap: your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
     */
    get_order_status(token1, token2, poolId, orderId, accountCap) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            txb.moveCall({
                typeArguments: [token1, token2],
                target: `dee9::clob_v2::get_order_status`,
                arguments: [txb.object(`${poolId}`), txb.object(String(orderId)), txb.object(`${accountCap}`)],
            });
            txb.setSender(this.currentAddress);
            return yield this.provider.devInspectTransactionBlock({
                transactionBlock: txb,
                sender: this.currentAddress,
            });
        });
    }
    /**
     * @description: get the base and quote token in custodian account
     * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
     * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
     * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
     * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
     */
    get_usr_position(token1, token2, poolId, accountCap) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            txb.moveCall({
                typeArguments: [token1, token2],
                target: `dee9::clob_v2::account_balance`,
                arguments: [txb.object(`${poolId}`), txb.object(`${accountCap}`)],
            });
            txb.setSender(this.currentAddress);
            return yield this.provider.devInspectTransactionBlock({
                transactionBlock: txb,
                sender: this.currentAddress,
            });
        });
    }
    /**
     * @description get the open orders of the current user
     * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
     * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
     * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
     * @param accountCap your accountCap, eg: 0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3
     */
    list_open_orders(token1, token2, poolId, accountCap) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            txb.moveCall({
                typeArguments: [token1, token2],
                target: `dee9::clob_v2::list_open_orders`,
                arguments: [txb.object(`${poolId}`), txb.object(`${accountCap}`)],
            });
            txb.setSender(this.currentAddress);
            return yield this.provider.devInspectTransactionBlock({
                transactionBlock: txb,
                sender: this.currentAddress,
            });
        });
    }
    /**
     * @description get the market price {bestBidPrice, bestAskPrice}
     * @param token1 token1 of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
     * @param token2 token2 of a certain pair,  eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
     * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
     */
    get_market_price(token1, token2, poolId) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            txb.moveCall({
                typeArguments: [token1, token2],
                target: `dee9::clob_v2::get_market_price`,
                arguments: [txb.object(`${poolId}`)],
            });
            return yield this.provider.devInspectTransactionBlock({
                transactionBlock: txb,
                sender: this.currentAddress,
            });
        });
    }
    /**
     * @description get level2 book status
     * @param token1 token1 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH
     * @param token2 token2 of a certain pair, eg: 0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT
     * @param poolId the pool id, eg: 0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4
     * @param lowerPrice lower price you want to query in the level2 book, eg: 18000000000
     * @param higherPrice higher price you want to query in the level2 book, eg: 20000000000
     * @param is_bid_side true: query bid side, false: query ask side
     */
    get_level2_book_status(token1, token2, poolId, lowerPrice, higherPrice, is_bid_side) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            txb.moveCall({
                typeArguments: [token1, token2],
                target: is_bid_side
                    ? `dee9::clob_v2::get_level2_book_status_bid_side`
                    : `dee9::clob_v2::get_level2_book_status_ask_side`,
                arguments: [
                    txb.object(`${poolId}`),
                    txb.pure(String(lowerPrice)),
                    txb.pure(String(higherPrice)),
                    txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
                ],
            });
            return yield this.provider.devInspectTransactionBlock({
                transactionBlock: txb,
                sender: this.currentAddress,
            });
        });
    }
}
exports.DeepBook_query = DeepBook_query;
