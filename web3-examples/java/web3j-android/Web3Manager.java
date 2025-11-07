package com.example.web3android;

import org.web3j.abi.FunctionEncoder;
import org.web3j.abi.FunctionReturnDecoder;
import org.web3j.abi.TypeReference;
import org.web3j.abi.datatypes.Address;
import org.web3j.abi.datatypes.Function;
import org.web3j.abi.datatypes.Type;
import org.web3j.abi.datatypes.generated.Uint256;
import org.web3j.crypto.Credentials;
import org.web3j.protocol.Web3j;
import org.web3j.protocol.core.DefaultBlockParameterName;
import org.web3j.protocol.core.methods.request.Transaction;
import org.web3j.protocol.core.methods.response.*;
import org.web3j.protocol.http.HttpService;
import org.web3j.tx.RawTransactionManager;
import org.web3j.tx.gas.DefaultGasProvider;
import org.web3j.utils.Convert;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;

/**
 * Web3 Manager for Android
 * Comprehensive Web3j integration for Android applications
 */
public class Web3Manager {

    private final Web3j web3j;
    private final String rpcUrl;
    private Credentials credentials;

    /**
     * Create Web3Manager instance
     * @param rpcUrl RPC endpoint URL
     */
    public Web3Manager(String rpcUrl) {
        this.rpcUrl = rpcUrl;
        this.web3j = Web3j.build(new HttpService(rpcUrl));
    }

    /**
     * Set wallet credentials
     * @param privateKey Private key hex string
     */
    public void setCredentials(String privateKey) {
        this.credentials = Credentials.create(privateKey);
    }

    /**
     * Get ETH balance of address
     * @param address Ethereum address
     * @return Balance in ETH
     */
    public CompletableFuture<BigDecimal> getBalance(String address) {
        return web3j.ethGetBalance(address, DefaultBlockParameterName.LATEST)
                .sendAsync()
                .thenApply(EthGetBalance::getBalance)
                .thenApply(balance -> Convert.fromWei(
                        new BigDecimal(balance),
                        Convert.Unit.ETHER
                ));
    }

    /**
     * Get current block number
     * @return Block number
     */
    public CompletableFuture<BigInteger> getBlockNumber() {
        return web3j.ethBlockNumber()
                .sendAsync()
                .thenApply(EthBlockNumber::getBlockNumber);
    }

    /**
     * Get transaction by hash
     * @param txHash Transaction hash
     * @return Transaction object
     */
    public CompletableFuture<org.web3j.protocol.core.methods.response.Transaction> getTransaction(String txHash) {
        return web3j.ethGetTransactionByHash(txHash)
                .sendAsync()
                .thenApply(EthTransaction::getTransaction)
                .thenApply(opt -> opt.orElse(null));
    }

    /**
     * Get transaction receipt
     * @param txHash Transaction hash
     * @return Transaction receipt
     */
    public CompletableFuture<TransactionReceipt> getTransactionReceipt(String txHash) {
        return web3j.ethGetTransactionReceipt(txHash)
                .sendAsync()
                .thenApply(EthGetTransactionReceipt::getTransactionReceipt)
                .thenApply(opt -> opt.orElse(null));
    }

    /**
     * Send ETH transaction
     * @param toAddress Recipient address
     * @param amountEth Amount in ETH
     * @return Transaction hash
     */
    public CompletableFuture<String> sendEth(String toAddress, BigDecimal amountEth) throws Exception {
        if (credentials == null) {
            throw new IllegalStateException("Credentials not set");
        }

        BigInteger amountWei = Convert.toWei(amountEth, Convert.Unit.ETHER).toBigInteger();

        RawTransactionManager txManager = new RawTransactionManager(web3j, credentials);

        return txManager.sendEthTransaction(
                toAddress,
                amountWei,
                DefaultGasProvider.GAS_PRICE,
                DefaultGasProvider.GAS_LIMIT
        ).sendAsync().thenApply(EthSendTransaction::getTransactionHash);
    }

    /**
     * ERC20 Token Operations
     */
    public static class ERC20 {
        private final Web3j web3j;
        private final String contractAddress;

        public ERC20(Web3j web3j, String contractAddress) {
            this.web3j = web3j;
            this.contractAddress = contractAddress;
        }

        /**
         * Get token balance
         * @param ownerAddress Owner address
         * @return Token balance
         */
        public CompletableFuture<BigInteger> balanceOf(String ownerAddress) {
            Function function = new Function(
                    "balanceOf",
                    Arrays.asList(new Address(ownerAddress)),
                    Arrays.asList(new TypeReference<Uint256>() {})
            );

            String encodedFunction = FunctionEncoder.encode(function);

            return web3j.ethCall(
                    Transaction.createEthCallTransaction(
                            ownerAddress,
                            contractAddress,
                            encodedFunction
                    ),
                    DefaultBlockParameterName.LATEST
            ).sendAsync().thenApply(response -> {
                List<Type> result = FunctionReturnDecoder.decode(
                        response.getValue(),
                        function.getOutputParameters()
                );
                return (BigInteger) result.get(0).getValue();
            });
        }

        /**
         * Get token name
         * @return Token name
         */
        public CompletableFuture<String> name() {
            Function function = new Function(
                    "name",
                    Collections.emptyList(),
                    Arrays.asList(new TypeReference<org.web3j.abi.datatypes.Utf8String>() {})
            );

            String encodedFunction = FunctionEncoder.encode(function);

            return web3j.ethCall(
                    Transaction.createEthCallTransaction(
                            null,
                            contractAddress,
                            encodedFunction
                    ),
                    DefaultBlockParameterName.LATEST
            ).sendAsync().thenApply(response -> {
                List<Type> result = FunctionReturnDecoder.decode(
                        response.getValue(),
                        function.getOutputParameters()
                );
                return (String) result.get(0).getValue();
            });
        }

        /**
         * Transfer tokens
         * @param credentials Sender credentials
         * @param toAddress Recipient address
         * @param amount Amount to transfer
         * @return Transaction hash
         */
        public CompletableFuture<String> transfer(
                Credentials credentials,
                String toAddress,
                BigInteger amount
        ) {
            Function function = new Function(
                    "transfer",
                    Arrays.asList(new Address(toAddress), new Uint256(amount)),
                    Collections.emptyList()
            );

            String encodedFunction = FunctionEncoder.encode(function);

            RawTransactionManager txManager = new RawTransactionManager(web3j, credentials);

            return txManager.sendTransaction(
                    DefaultGasProvider.GAS_PRICE,
                    DefaultGasProvider.GAS_LIMIT,
                    contractAddress,
                    encodedFunction,
                    BigInteger.ZERO
            ).sendAsync().thenApply(EthSendTransaction::getTransactionHash);
        }
    }

    /**
     * Create ERC20 instance
     * @param tokenAddress Token contract address
     * @return ERC20 instance
     */
    public ERC20 erc20(String tokenAddress) {
        return new ERC20(web3j, tokenAddress);
    }

    /**
     * Get gas price
     * @return Gas price in Wei
     */
    public CompletableFuture<BigInteger> getGasPrice() {
        return web3j.ethGasPrice()
                .sendAsync()
                .thenApply(EthGasPrice::getGasPrice);
    }

    /**
     * Get chain ID
     * @return Chain ID
     */
    public CompletableFuture<Long> getChainId() {
        return web3j.ethChainId()
                .sendAsync()
                .thenApply(EthChainId::getChainId)
                .thenApply(BigInteger::longValue);
    }

    /**
     * Close connection
     */
    public void shutdown() {
        web3j.shutdown();
    }
}
