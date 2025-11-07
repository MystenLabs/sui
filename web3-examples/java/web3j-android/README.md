# Web3j Android SDK

Professional Web3 integration for Android applications using Web3j.

## Features

- ✅ ETH balance queries
- ✅ Send transactions
- ✅ ERC20 token support
- ✅ Contract interactions
- ✅ Block and transaction queries
- ✅ Gas price estimation
- ✅ Async/CompletableFuture API
- ✅ Android-optimized

## Setup

### Add to build.gradle

```gradle
dependencies {
    implementation 'org.web3j:core:4.10.3'
}
```

### Permissions (AndroidManifest.xml)

```xml
<uses-permission android:name="android.permission.INTERNET" />
```

## Usage

### Initialize Web3Manager

```java
Web3Manager web3 = new Web3Manager("https://eth.llamarpc.com");
```

### Get Balance

```java
web3.getBalance("0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb8")
    .thenAccept(balance -> {
        Log.d("Balance", balance + " ETH");
    });
```

### Send Transaction

```java
web3.setCredentials("PRIVATE_KEY");

web3.sendEth("0xRecipient...", new BigDecimal("0.1"))
    .thenAccept(txHash -> {
        Log.d("Transaction", "Hash: " + txHash);
    });
```

### ERC20 Token Operations

```java
Web3Manager.ERC20 token = web3.erc20("0xTokenAddress...");

// Get balance
token.balanceOf("0xOwner...")
    .thenAccept(balance -> {
        Log.d("Token Balance", balance.toString());
    });

// Transfer tokens
Credentials credentials = Credentials.create("PRIVATE_KEY");
token.transfer(credentials, "0xRecipient...", BigInteger.valueOf(100))
    .thenAccept(txHash -> {
        Log.d("Transfer", "Hash: " + txHash);
    });
```

### Get Block Number

```java
web3.getBlockNumber()
    .thenAccept(blockNumber -> {
        Log.d("Block", "Current: " + blockNumber);
    });
```

### Get Transaction

```java
web3.getTransaction("0xTxHash...")
    .thenAccept(tx -> {
        if (tx != null) {
            Log.d("TX", "From: " + tx.getFrom());
            Log.d("TX", "To: " + tx.getTo());
        }
    });
```

### Get Gas Price

```java
web3.getGasPrice()
    .thenAccept(gasPrice -> {
        BigInteger gasPriceGwei = gasPrice.divide(BigInteger.valueOf(1_000_000_000));
        Log.d("Gas", gasPriceGwei + " Gwei");
    });
```

## Kotlin Example

```kotlin
class MainActivity : AppCompatActivity() {
    private lateinit var web3: Web3Manager

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        web3 = Web3Manager("https://eth.llamarpc.com")

        lifecycleScope.launch {
            val balance = web3.getBalance("0x...").await()
            textView.text = "Balance: $balance ETH"
        }
    }
}
```

## Error Handling

```java
web3.getBalance(address)
    .exceptionally(throwable -> {
        Log.e("Error", "Failed to get balance", throwable);
        return BigDecimal.ZERO;
    })
    .thenAccept(balance -> {
        // Handle balance
    });
```

## Best Practices

1. **Never hardcode private keys**
   ```java
   // Use secure storage
   String privateKey = secureStorage.getPrivateKey();
   web3.setCredentials(privateKey);
   ```

2. **Handle network errors**
   ```java
   if (!NetworkUtils.isConnected(context)) {
       showError("No internet connection");
       return;
   }
   ```

3. **Run on background thread**
   ```java
   // CompletableFuture runs async by default
   web3.getBalance(address).thenAccept(/* ... */);
   ```

4. **Validate addresses**
   ```java
   if (!WalletUtils.isValidAddress(address)) {
       showError("Invalid address");
       return;
   }
   ```

## Security

- Store private keys in Android Keystore
- Use ProGuard/R8 for code obfuscation
- Validate all user inputs
- Use HTTPS for RPC endpoints
- Implement proper error handling

## Architecture

```
┌──────────────────┐
│  Android App     │
│  (UI Layer)      │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Web3Manager     │
│  (Business)      │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  Web3j Library   │
│  (Core)          │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  RPC Provider    │
│  (Network)       │
└──────────────────┘
```

## Sample App Structure

```
app/
├── src/
│   ├── main/
│   │   ├── java/com/example/web3android/
│   │   │   ├── Web3Manager.java
│   │   │   ├── MainActivity.java
│   │   │   └── WalletActivity.java
│   │   ├── res/
│   │   └── AndroidManifest.xml
│   └── test/
└── build.gradle
```

## Dependencies

- `web3j:core` 4.10.3
- Android SDK 24+
- Java 11+

## Resources

- [Web3j Documentation](https://docs.web3j.io/)
- [Android Developers](https://developer.android.com/)
- [Ethereum for Mobile](https://ethereum.org/en/developers/docs/programming-languages/java/)
