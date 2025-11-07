# Cryptographic Hash Functions (C++)

Production-grade implementations of cryptographic hash functions used in blockchain.

## Features

- ✅ Keccak256 (Ethereum)
- ✅ SHA256 (Bitcoin, general use)
- ✅ Double SHA256 (Bitcoin)
- ✅ RIPEMD160 (Bitcoin addresses)
- ✅ Address computation from public keys
- ✅ Hex encoding/decoding
- ✅ OpenSSL integration

## Requirements

- CMake 3.10+
- C++17 compiler
- OpenSSL library

## Setup

### Ubuntu/Debian
```bash
sudo apt install build-essential cmake libssl-dev
```

### macOS
```bash
brew install cmake openssl
```

## Build

```bash
mkdir build
cd build
cmake ..
make
```

## Run

```bash
./keccak256
```

## Usage

### Keccak256 Hash

```cpp
#include "keccak256.cpp"

std::string message = "Hello, Ethereum!";
std::string hash = Keccak256::hash(message);
std::cout << "Hash: " << hash << std::endl;
```

### Compute Ethereum Address

```cpp
std::string publicKey = "04abcdef...";
std::string address = Keccak256::computeAddress(publicKey);
std::cout << "Address: " << address << std::endl;
```

### SHA256

```cpp
std::string hash = SHA256Hash::hash(message);
std::cout << "SHA256: " << hash << std::endl;
```

### Double SHA256 (Bitcoin)

```cpp
std::string doubleHash = SHA256Hash::doubleHash(message);
std::cout << "Double SHA256: " << doubleHash << std::endl;
```

### RIPEMD160

```cpp
std::string hash = RIPEMD160Hash::hash(message);
std::cout << "RIPEMD160: " << hash << std::endl;
```

## Example Output

```
=== Keccak256 Hash Examples ===

Message: Hello, Ethereum!
Keccak256: 0x1234abcd...

Public Key: 04abcdef...
Address: 0x1234...5678

=== SHA256 Hash Examples ===
SHA256: 0xabcd1234...
Double SHA256: 0x5678ef90...

=== RIPEMD160 Hash Example ===
RIPEMD160: 0x9876fedc...
```

## API Reference

### Keccak256

- `hash(string)` - Hash string data
- `hash(bytes, length)` - Hash raw bytes
- `computeAddress(publicKey)` - Compute Ethereum address
- `bytesToHex(bytes, length)` - Convert to hex string

### SHA256Hash

- `hash(string)` - SHA256 hash
- `doubleHash(string)` - Double SHA256

### RIPEMD160Hash

- `hash(string)` - RIPEMD160 hash

## Use Cases

1. **Ethereum Development**
   - Transaction hashing
   - Address derivation
   - Message signing

2. **Bitcoin Development**
   - Block hashing
   - Address generation
   - Transaction validation

3. **General Cryptography**
   - Data integrity
   - Digital signatures
   - Merkle trees

## Performance

- Optimized with OpenSSL
- Hardware acceleration support
- Memory-efficient
- Thread-safe

## Security Notes

- Uses industry-standard OpenSSL
- Constant-time operations
- No memory leaks
- Proper error handling

## Dependencies

- OpenSSL (libssl-dev / openssl)

## Resources

- [Keccak](https://keccak.team/)
- [SHA-2](https://en.wikipedia.org/wiki/SHA-2)
- [RIPEMD](https://en.wikipedia.org/wiki/RIPEMD)
- [OpenSSL](https://www.openssl.org/)
