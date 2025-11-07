package main

import (
	"crypto/ecdsa"
	"fmt"
	"log"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/crypto"
)

// SignatureVerifier handles Ethereum signature operations
type SignatureVerifier struct{}

// NewSignatureVerifier creates a new verifier instance
func NewSignatureVerifier() *SignatureVerifier {
	return &SignatureVerifier{}
}

// GenerateKeyPair generates a new ECDSA key pair
func (sv *SignatureVerifier) GenerateKeyPair() (*ecdsa.PrivateKey, error) {
	privateKey, err := crypto.GenerateKey()
	if err != nil {
		return nil, fmt.Errorf("failed to generate key: %w", err)
	}

	publicKey := privateKey.Public()
	publicKeyECDSA, ok := publicKey.(*ecdsa.PublicKey)
	if !ok {
		return nil, fmt.Errorf("error casting public key to ECDSA")
	}

	address := crypto.PubkeyToAddress(*publicKeyECDSA)

	fmt.Println("New Key Pair Generated:")
	fmt.Printf("Address: %s\n", address.Hex())
	fmt.Printf("Private Key: %s\n", hexutil.Encode(crypto.FromECDSA(privateKey)))

	return privateKey, nil
}

// SignMessage signs a message with the provided private key
func (sv *SignatureVerifier) SignMessage(message string, privateKey *ecdsa.PrivateKey) ([]byte, error) {
	// Hash the message
	messageHash := crypto.Keccak256Hash([]byte(message))
	fmt.Printf("Message Hash: %s\n", messageHash.Hex())

	// Sign the hash
	signature, err := crypto.Sign(messageHash.Bytes(), privateKey)
	if err != nil {
		return nil, fmt.Errorf("failed to sign message: %w", err)
	}

	fmt.Printf("Signature: %s\n", hexutil.Encode(signature))
	return signature, nil
}

// VerifySignature verifies a signature against a message and address
func (sv *SignatureVerifier) VerifySignature(message string, signature []byte, expectedAddress common.Address) (bool, error) {
	// Hash the message
	messageHash := crypto.Keccak256Hash([]byte(message))

	// Remove the recovery byte (last byte) for verification
	if len(signature) != 65 {
		return false, fmt.Errorf("invalid signature length: %d", len(signature))
	}

	// Recover the public key from signature
	publicKey, err := crypto.SigToPub(messageHash.Bytes(), signature)
	if err != nil {
		return false, fmt.Errorf("failed to recover public key: %w", err)
	}

	// Get the address from public key
	recoveredAddress := crypto.PubkeyToAddress(*publicKey)

	fmt.Printf("Expected Address: %s\n", expectedAddress.Hex())
	fmt.Printf("Recovered Address: %s\n", recoveredAddress.Hex())

	return recoveredAddress == expectedAddress, nil
}

// PersonalSign creates an Ethereum personal_sign signature
func (sv *SignatureVerifier) PersonalSign(message string, privateKey *ecdsa.PrivateKey) ([]byte, error) {
	// Add Ethereum prefix
	prefix := fmt.Sprintf("\x19Ethereum Signed Message:\n%d%s", len(message), message)
	messageHash := crypto.Keccak256Hash([]byte(prefix))

	// Sign
	signature, err := crypto.Sign(messageHash.Bytes(), privateKey)
	if err != nil {
		return nil, fmt.Errorf("failed to sign message: %w", err)
	}

	// Adjust V value for Ethereum compatibility
	signature[64] += 27

	fmt.Printf("Personal Sign Signature: %s\n", hexutil.Encode(signature))
	return signature, nil
}

// VerifyPersonalSign verifies an Ethereum personal_sign signature
func (sv *SignatureVerifier) VerifyPersonalSign(message string, signature []byte) (common.Address, error) {
	// Add Ethereum prefix
	prefix := fmt.Sprintf("\x19Ethereum Signed Message:\n%d%s", len(message), message)
	messageHash := crypto.Keccak256Hash([]byte(prefix))

	// Adjust V value back
	if signature[64] >= 27 {
		signature[64] -= 27
	}

	// Recover public key
	publicKey, err := crypto.SigToPub(messageHash.Bytes(), signature)
	if err != nil {
		return common.Address{}, fmt.Errorf("failed to recover public key: %w", err)
	}

	address := crypto.PubkeyToAddress(*publicKey)
	return address, nil
}

// RecoverAddress recovers the address from a message and signature
func (sv *SignatureVerifier) RecoverAddress(messageHash common.Hash, signature []byte) (common.Address, error) {
	publicKey, err := crypto.SigToPub(messageHash.Bytes(), signature)
	if err != nil {
		return common.Address{}, fmt.Errorf("failed to recover public key: %w", err)
	}

	return crypto.PubkeyToAddress(*publicKey), nil
}

func main() {
	verifier := NewSignatureVerifier()

	// Generate key pair
	fmt.Println("=== Generate Key Pair ===")
	privateKey, err := verifier.GenerateKeyPair()
	if err != nil {
		log.Fatal(err)
	}

	publicKey := privateKey.Public().(*ecdsa.PublicKey)
	address := crypto.PubkeyToAddress(*publicKey)

	fmt.Println("\n=== Sign Message ===")
	message := "Hello, Ethereum!"
	signature, err := verifier.SignMessage(message, privateKey)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("\n=== Verify Signature ===")
	valid, err := verifier.VerifySignature(message, signature, address)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Signature Valid: %t\n", valid)

	fmt.Println("\n=== Personal Sign ===")
	personalSig, err := verifier.PersonalSign(message, privateKey)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println("\n=== Verify Personal Sign ===")
	recoveredAddr, err := verifier.VerifyPersonalSign(message, personalSig)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Recovered Address: %s\n", recoveredAddr.Hex())
	fmt.Printf("Match: %t\n", recoveredAddr == address)
}
