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
exports.DeepBook_sdk = void 0;
const sui_js_1 = require("@mysten/sui.js");
const utils_1 = require("./utils");
const utils_2 = require("./utils");
class DeepBook_sdk {
    constructor(provider = new sui_js_1.JsonRpcProvider(sui_js_1.localnetConnection), gasBudget, records) {
        this.provider = provider;
        this.gasBudget = gasBudget;
        this.records = records;
    }
    /**
     * @description: Create pool for trading pair
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param tickSize Minimal Price Change Accuracy of this pool, eg: 10000000
     * @param lotSize Minimal Lot Change Accuracy of this pool, eg: 10000
     */
    createPool(token1, token2, tickSize, lotSize) {
        const txb = new sui_js_1.TransactionBlock();
        // 100 sui to create a pool
        const [coin] = txb.splitCoins(txb.gas, [txb.pure(100000000000)]);
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::create_pool`,
            arguments: [txb.pure(`${tickSize}`), txb.pure(`${lotSize}`), coin],
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: Create and Transfer custodian account to user
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     */
    createAccount(currentAddress) {
        const txb = new sui_js_1.TransactionBlock();
        let [cap] = txb.moveCall({
            typeArguments: [],
            target: `dee9::clob_v2::create_account`,
            arguments: [],
        });
        txb.transferObjects([cap], txb.pure(currentAddress));
        txb.setSenderIfNotSet(currentAddress);
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: Deposit base asset into custodian account
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param coin Object id of coin to deposit, eg: "0x316467544c7e719384579ac5745c75be5984ca9f004d6c09fd7ca24e4d8a3d14"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    depositBase(token1, token2, poolId, coin, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::deposit_base`,
            arguments: [txb.object(`${poolId}`), txb.object(coin), txb.object(`${accountCap}`)],
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: Deposit quote asset into custodian account
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param coin: Object id of coin to deposit, eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    depositQuote(token1, token2, poolId, coin, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::deposit_quote`,
            arguments: [txb.object(`${poolId}`), txb.object(coin), txb.object(`${accountCap}`)],
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: Withdraw base asset from custodian account
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param quantity Amount of base asset to withdraw, eg: 10000000
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    withdrawBase(token1, token2, poolId, quantity, currentAddress, accountCap) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            const withdraw = txb.moveCall({
                typeArguments: [token1, token2],
                target: `dee9::clob_v2::withdraw_base`,
                arguments: [txb.object(`${poolId}`), txb.pure(quantity), txb.object(`${accountCap}`)],
            });
            txb.transferObjects([withdraw], txb.pure(currentAddress));
            txb.setGasBudget(this.gasBudget);
            return txb;
        });
    }
    /**
     * @description: Withdraw quote asset from custodian account
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param quantity Amount of base asset to withdraw, eg: 10000000
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    withdrawQuote(token1, token2, poolId, quantity, currentAddress, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        const withdraw = txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::withdraw_quote`,
            arguments: [txb.object(`${poolId}`), txb.pure(quantity), txb.object(`${accountCap}`)],
        });
        txb.transferObjects([withdraw], txb.pure(currentAddress));
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: swap exact quote for base
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param client_order_id an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param quantity Amount of quote asset to swap in base asset
     * @param is_bid true if the order is bid, false if the order is ask
     * @param baseCoin the objectId of the base coin
     * @param quoteCoin the objectId of the quote coin
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    place_market_order(token1, token2, client_order_id, poolId, quantity, is_bid, baseCoin, quoteCoin, currentAddress, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        const [base_coin_ret, quote_coin_ret] = txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::place_market_order`,
            arguments: [
                txb.object(`${poolId}`),
                txb.object(`${accountCap}`),
                txb.pure(client_order_id),
                txb.pure(quantity),
                txb.pure(is_bid),
                txb.object(baseCoin),
                txb.object(quoteCoin),
                txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
            ],
        });
        txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
        txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
        txb.setSenderIfNotSet(currentAddress);
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: swap exact quote for base
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param client_order_id an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param tokenObjectIn: Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
     * @param amountIn: amount of token to buy or sell, eg: 10000000
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    swap_exact_quote_for_base(token1, token2, client_order_id, poolId, tokenObjectIn, amountIn, currentAddress, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        // in this case, we assume that the tokenIn--tokenOut always exists.
        const [base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::swap_exact_quote_for_base`,
            arguments: [
                txb.object(`${poolId}`),
                txb.pure(client_order_id),
                txb.object(`${accountCap}`),
                txb.object(String(amountIn)),
                txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
                txb.object(tokenObjectIn),
            ],
        });
        txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
        txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
        txb.setSenderIfNotSet(currentAddress);
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description swap exact base for quote
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param client_order_id an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param treasury treasury of the quote coin, in the selling case, we will mint a zero quote coin to receive the quote coin from the pool. eg: "0x0a11d301013759e79cb5f89a8bb29c3f9a9bb5be6dec2ddba48ea4b39abc5b5a"
     * @param tokenObjectIn Object id of the token to swap: eg: "0x6e566fec4c388eeb78a7dab832c9f0212eb2ac7e8699500e203def5b41b9c70d"
     * @param amountIn amount of token to buy or sell, eg: 10000000
     * @param currentAddress current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    swap_exact_base_for_quote(token1, token2, client_order_id, poolId, treasury, tokenObjectIn, amountIn, currentAddress, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        // in this case, we assume that the tokenIn--tokenOut always exists.
        const [base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::swap_exact_base_for_quote`,
            arguments: [
                txb.object(`${poolId}`),
                txb.pure(client_order_id),
                txb.object(`${accountCap}`),
                txb.object(String(amountIn)),
                txb.object(tokenObjectIn),
                txb.moveCall({
                    typeArguments: [token2],
                    target: `0x2::coin::zero`,
                    arguments: [],
                }),
                txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
            ],
        });
        txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
        txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
        txb.setSenderIfNotSet(currentAddress);
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: place a limit order
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param client_order_id
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param price: price of the limit order, eg: 180000000
     * @param quantity: quantity of the limit order in BASE ASSET, eg: 100000000
     * @param isBid: true for buying base with quote, false for selling base for quote
     * @param expireTimestamp: expire timestamp of the limit order in ms, eg: 1620000000000
     * @param restriction restrictions on limit orders, explain in doc for more details, eg: 0
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    placeLimitOrder(token1, token2, client_order_id, poolId, price, quantity, isBid, expireTimestamp, restriction, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        const args = [
            txb.object(`${poolId}`),
            txb.pure(client_order_id),
            txb.pure(Math.floor(price * 1000000000)),
            txb.pure(quantity),
            txb.pure(isBid),
            txb.pure(expireTimestamp),
            txb.pure(restriction),
            txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
            txb.object(`${accountCap}`),
        ];
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::place_limit_order`,
            arguments: args,
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: cancel an order
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param orderId orderId of a limit order, you can find them through function query.list_open_orders eg: "0"
     * @param currentAddress: current user address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    cancelOrder(token1, token2, poolId, orderId, currentAddress, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::cancel_order`,
            arguments: [txb.object(`${poolId}`), txb.pure(orderId), txb.object(`${accountCap}`)],
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: Cancel all limit orders under a certain account capacity
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    cancelAllOrders(token1, token2, poolId, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::cancel_all_orders`,
            arguments: [txb.object(`${poolId}`), txb.object(`${accountCap}`)],
        });
        txb.setGasBudget(this.gasBudget);
        return txb;
    }
    /**
     * @description: batch cancel order
     * @param token1 Full coin type of the base asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::wbtc::WBTC"
     * @param token2 Full coin type of quote asset, eg: "0x3d0d0ce17dcd3b40c2d839d96ce66871ffb40e1154a8dd99af72292b3d10d7fc::usdt::USDT"
     * @param poolId Object id of pool, created after invoking createPool, eg: "0xcaee8e1c046b58e55196105f1436a2337dcaa0c340a7a8c8baf65e4afb8823a4"
     * @param orderIds array of order ids you want to cancel, you can find your open orders by query.list_open_orders eg: ["0", "1", "2"]
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     */
    batchCancelOrder(token1, token2, poolId, orderIds, accountCap) {
        const txb = new sui_js_1.TransactionBlock();
        txb.moveCall({
            typeArguments: [token1, token2],
            target: `dee9::clob_v2::batch_cancel_order`,
            arguments: [txb.object(`${poolId}`), txb.pure(orderIds), txb.object(`${accountCap}`)],
        });
        txb.setGasBudget(utils_2.defaultGasBudget);
        return txb;
    }
    /**
     * @param tokenInObject the tokenObject you want to swap
     * @param tokenOut the token you want to swap to
     * @param client_order_id an id which identify who make the order, you can define it by yourself, eg: "1" , "2", ...
     * @param amountIn the amount of token you want to swap
     * @param isBid true for bid, false for ask
     * @param currentAddress current user address
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount
     */
    findBestRoute(tokenInObject, tokenOut, client_order_id, amountIn, isBid, currentAddress, accountCap) {
        var _a;
        return __awaiter(this, void 0, void 0, function* () {
            // const tokenTypeIn: string = convertToTokenType(tokenIn, this.records);
            // should get the tokenTypeIn from tokenInObject
            const tokenInfo = yield this.provider.getObject({
                id: tokenInObject,
                options: {
                    showType: true,
                },
            });
            if (!((_a = tokenInfo === null || tokenInfo === void 0 ? void 0 : tokenInfo.data) === null || _a === void 0 ? void 0 : _a.type)) {
                throw new Error(`token ${tokenInObject} not found`);
            }
            const tokenTypeIn = tokenInfo.data.type.split('<')[1].split('>')[0];
            const paths = this.dfs(tokenTypeIn, tokenOut, this.records);
            let maxSwapTokens = 0;
            let smartRoute = [];
            for (const path of paths) {
                const smartRouteResultWithExactPath = yield this.placeMarketOrderWithSmartRouting(tokenInObject, tokenOut, client_order_id, isBid, amountIn, currentAddress, accountCap, path);
                if (smartRouteResultWithExactPath && smartRouteResultWithExactPath.amount > maxSwapTokens) {
                    maxSwapTokens = smartRouteResultWithExactPath.amount;
                    smartRoute = path;
                }
            }
            return { maxSwapTokens, smartRoute };
        });
    }
    /**
     * @param tokenInObject the tokenObject you want to swap
     * @param tokenTypeOut the token type you want to swap to
     * @param client_order_id the client order id
     * @param isBid true for bid, false for ask
     * @param amountIn the amount of token you want to swap: eg, 1000000
     * @param currentAddress your own address, eg: "0xbddc9d4961b46a130c2e1f38585bbc6fa8077ce54bcb206b26874ac08d607966"
     * @param accountCap Object id of Account Capacity under user address, created after invoking createAccount, eg: "0x6f699fef193723277559c8f499ca3706121a65ac96d273151b8e52deb29135d3"
     * @param path the path you want to swap through, for example, you have found that the best route is wbtc --> usdt --> weth, then the path should be ["0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::wbtc::WBTC", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::usdt::USDT", "0x5378a0e7495723f7d942366a125a6556cf56f573fa2bb7171b554a2986c4229a::weth::WETH"]
     */
    placeMarketOrderWithSmartRouting(tokenInObject, tokenTypeOut, client_order_id, isBid, amountIn, currentAddress, accountCap, path) {
        return __awaiter(this, void 0, void 0, function* () {
            const txb = new sui_js_1.TransactionBlock();
            const tokenIn = txb.object(tokenInObject);
            txb.setGasBudget(this.gasBudget);
            txb.setSenderIfNotSet(currentAddress);
            let i = 0;
            let base_coin_ret;
            let quote_coin_ret;
            let amount;
            let lastBid;
            while (path[i]) {
                const nextPath = path[i + 1] ? path[i + 1] : tokenTypeOut;
                const poolInfo = (0, utils_1.getPoolInfoByRecords)(path[i], nextPath, this.records);
                let _isBid, _tokenIn, _tokenOut, _amount;
                if (i == 0) {
                    if (!isBid) {
                        _isBid = false;
                        _tokenIn = tokenIn;
                        _tokenOut = txb.moveCall({
                            typeArguments: [nextPath],
                            target: `0x2::coin::zero`,
                            arguments: [],
                        });
                        _amount = txb.object(String(amountIn));
                    }
                    else {
                        _isBid = true;
                        // _tokenIn = this.mint(txb, nextPath, 0)
                        _tokenOut = tokenIn;
                        _amount = txb.object(String(amountIn));
                    }
                }
                else {
                    if (!isBid) {
                        txb.transferObjects(
                        // @ts-ignore
                        [lastBid ? quote_coin_ret : base_coin_ret], txb.pure(currentAddress));
                        _isBid = false;
                        // @ts-ignore
                        _tokenIn = lastBid ? base_coin_ret : quote_coin_ret;
                        _tokenOut = txb.moveCall({
                            typeArguments: [nextPath],
                            target: `0x2::coin::zero`,
                            arguments: [],
                        });
                        // @ts-ignore
                        _amount = amount;
                    }
                    else {
                        txb.transferObjects(
                        // @ts-ignore
                        [lastBid ? quote_coin_ret : base_coin_ret], txb.pure(currentAddress));
                        _isBid = true;
                        // _tokenIn = this.mint(txb, nextPath, 0)
                        // @ts-ignore
                        _tokenOut = lastBid ? base_coin_ret : quote_coin_ret;
                        // @ts-ignore
                        _amount = amount;
                    }
                }
                lastBid = _isBid;
                // in this moveCall we will change to swap_exact_base_for_quote
                // if isBid, we will use swap_exact_quote_for_base
                // is !isBid, we will use swap_exact_base_for_quote
                if (_isBid) {
                    // here swap_exact_quote_for_base
                    [base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
                        typeArguments: [isBid ? nextPath : path[i], isBid ? path[i] : nextPath],
                        target: `dee9::clob_v2::swap_exact_quote_for_base`,
                        arguments: [
                            txb.object(String(poolInfo.clob_v2)),
                            txb.pure(String(client_order_id)),
                            txb.object(String(accountCap)),
                            _amount,
                            txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
                            _tokenOut,
                        ],
                    });
                }
                else {
                    // here swap_exact_base_for_quote
                    [base_coin_ret, quote_coin_ret, amount] = txb.moveCall({
                        typeArguments: [isBid ? nextPath : path[i], isBid ? path[i] : nextPath],
                        target: `dee9::clob_v2::swap_exact_base_for_quote`,
                        arguments: [
                            txb.object(String(poolInfo.clob_v2)),
                            txb.pure(String(client_order_id)),
                            txb.object(String(accountCap)),
                            _amount,
                            // @ts-ignore
                            _tokenIn,
                            _tokenOut,
                            txb.object((0, sui_js_1.normalizeSuiObjectId)('0x6')),
                        ],
                    });
                }
                if (nextPath == tokenTypeOut) {
                    txb.transferObjects([base_coin_ret], txb.pure(currentAddress));
                    txb.transferObjects([quote_coin_ret], txb.pure(currentAddress));
                    break;
                }
                else {
                    i += 1;
                }
            }
            const r = yield this.provider.dryRunTransactionBlock({
                transactionBlock: yield txb.build({
                    provider: this.provider,
                }),
            });
            if (r.effects.status.status === 'success') {
                for (const ele of r.balanceChanges) {
                    if (ele.coinType == tokenTypeOut) {
                        return {
                            txb: txb,
                            amount: Number(ele.amount),
                        };
                    }
                }
            }
        });
    }
    /**
     * @param tokenTypeIn the token type you want to swap with
     * @param tokenTypeOut the token type you want to swap to
     * @param records the pool records
     * @param path the path you want to swap through, in the first step, this path is [], then it will be a recursive function
     * @param depth the depth of the dfs, it is default to 2, which means, there will be a max of two steps of swap(say A-->B--C), but you can change it as you want lol
     * @param res the result of the dfs, in the first step, this res is [], then it will be a recursive function
     */
    dfs(tokenTypeIn, tokenTypeOut, records, path = [], depth = 2, res = new Array().fill([])) {
        // first updates the records
        if (depth < 0) {
            return res;
        }
        depth = depth - 1;
        if (tokenTypeIn === tokenTypeOut) {
            res.push(path);
            return [path];
        }
        // find children of tokenIn
        let children = new Set();
        for (const record of records.pools) {
            if (String(record.type).indexOf(tokenTypeIn.substring(2)) > -1) {
                String(record.type)
                    .split(',')
                    .forEach((token) => {
                    if (token.indexOf('clob_v2') != -1) {
                        token = token.split('<')[1];
                    }
                    else {
                        token = token.split('>')[0].substring(1);
                    }
                    if (token !== tokenTypeIn && path.indexOf(token) === -1) {
                        children.add(token);
                    }
                });
            }
        }
        children.forEach((child) => {
            const result = this.dfs(child, tokenTypeOut, records, [...path, tokenTypeIn], depth, res);
            if (result) {
                return result;
            }
        });
        return res;
    }
}
exports.DeepBook_sdk = DeepBook_sdk;
