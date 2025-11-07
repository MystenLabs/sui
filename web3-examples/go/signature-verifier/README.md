# Ethereum Signature Verifier (Go)

Cryptographic signature operations for Ethereum using Go.

## Features

- ✅ Generate ECDSA key pairs
- ✅ Sign messages
- ✅ Verify signatures
- ✅ Personal sign (Ethereum standard)
- ✅ Address recovery from signatures
- ✅ Keccak256 hashing

## Setup

```bash
go mod download
```

## Build

```bash
go build -o verifier main.go
```

## Run

```bash
go run main.go
```

## Usage

### Generate Key Pair

```go
verifier := NewSignatureVerifier()
privateKey, err := verifier.GenerateKeyPair()
```

### Sign Message

```go
message := "Hello, Ethereum!"
signature, err := verifier.SignMessage(message, privateKey)
```

### Verify Signature

```go
valid, err := verifier.VerifySignature(message, signature, address)
if valid {
    fmt.Println("Signature is valid!")
}
```

### Personal Sign

```go
personalSig, err := verifier.PersonalSign(message, privateKey)
recoveredAddr, err := verifier.VerifyPersonalSign(message, personalSig)
```

## Example Output

```
=== Generate Key Pair ===
New Key Pair Generated:
Address: 0x1234...
Private Key: 0xabcd...

=== Sign Message ===
Message Hash: 0x5678...
Signature: 0xef90...

=== Verify Signature ===
Expected Address: 0x1234...
Recovered Address: 0x1234...
Signature Valid: true
```

## API Reference

### SignatureVerifier Methods

- `GenerateKeyPair()` - Generate new ECDSA key pair
- `SignMessage(message, privateKey)` - Sign a message
- `VerifySignature(message, signature, address)` - Verify signature
- `PersonalSign(message, privateKey)` - Create personal_sign signature
- `VerifyPersonalSign(message, signature)` - Verify personal_sign
- `RecoverAddress(messageHash, signature)` - Recover address from signature

## Use Cases

1. **Wallet Development** - Sign transactions and messages
2. **Authentication** - Verify user ownership of addresses
3. **Off-chain Signatures** - Meta-transactions and permits
4. **Message Verification** - Verify signed messages

## Security Notes

- Keep private keys secure
- Never expose private keys in logs
- Use secure random number generation
- Validate all inputs

## Dependencies

- `go-ethereum` v1.13.5

## Resources

- [Go Ethereum](https://geth.ethereum.org/docs/dapp/native)
- [ECDSA](https://en.wikipedia.org/wiki/Elliptic_Curve_Digital_Signature_Algorithm)
- [EIP-191](https://eips.ethereum.org/EIPS/eip-191) - Signed Data Standard
