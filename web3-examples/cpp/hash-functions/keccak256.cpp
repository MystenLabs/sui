/**
 * Keccak256 Hash Implementation
 * Core cryptographic function used in Ethereum
 */

#include <iostream>
#include <iomanip>
#include <string>
#include <vector>
#include <cstring>
#include <openssl/evp.h>
#include <openssl/sha.h>

class Keccak256 {
public:
    /**
     * Compute Keccak256 hash of input data
     * @param data Input data to hash
     * @return 32-byte hash as hex string
     */
    static std::string hash(const std::string& data) {
        return hash(reinterpret_cast<const unsigned char*>(data.c_str()), data.length());
    }

    /**
     * Compute Keccak256 hash of raw bytes
     * @param data Raw input bytes
     * @param length Length of input
     * @return 32-byte hash as hex string
     */
    static std::string hash(const unsigned char* data, size_t length) {
        unsigned char hash[32];
        EVP_MD_CTX* context = EVP_MD_CTX_new();

        if (context == nullptr) {
            throw std::runtime_error("Failed to create hash context");
        }

        // Use SHA3-256 (Keccak256 variant)
        if (EVP_DigestInit_ex(context, EVP_sha3_256(), nullptr) != 1) {
            EVP_MD_CTX_free(context);
            throw std::runtime_error("Failed to initialize hash");
        }

        if (EVP_DigestUpdate(context, data, length) != 1) {
            EVP_MD_CTX_free(context);
            throw std::runtime_error("Failed to update hash");
        }

        unsigned int hash_length = 0;
        if (EVP_DigestFinal_ex(context, hash, &hash_length) != 1) {
            EVP_MD_CTX_free(context);
            throw std::runtime_error("Failed to finalize hash");
        }

        EVP_MD_CTX_free(context);

        return bytesToHex(hash, 32);
    }

    /**
     * Convert bytes to hexadecimal string
     */
    static std::string bytesToHex(const unsigned char* data, size_t length) {
        std::stringstream ss;
        ss << std::hex << std::setfill('0');
        for (size_t i = 0; i < length; i++) {
            ss << std::setw(2) << static_cast<unsigned>(data[i]);
        }
        return "0x" + ss.str();
    }

    /**
     * Compute Ethereum address from public key
     * Address = last 20 bytes of Keccak256(publicKey)
     */
    static std::string computeAddress(const std::string& publicKey) {
        std::string fullHash = hash(publicKey);
        // Take last 40 hex chars (20 bytes) and add 0x prefix
        return "0x" + fullHash.substr(fullHash.length() - 40);
    }
};

/**
 * SHA256 Hash (for Bitcoin and other chains)
 */
class SHA256Hash {
public:
    static std::string hash(const std::string& data) {
        unsigned char hash[SHA256_DIGEST_LENGTH];
        SHA256_CTX sha256;

        SHA256_Init(&sha256);
        SHA256_Update(&sha256, data.c_str(), data.length());
        SHA256_Final(hash, &sha256);

        std::stringstream ss;
        ss << std::hex << std::setfill('0');
        for (int i = 0; i < SHA256_DIGEST_LENGTH; i++) {
            ss << std::setw(2) << static_cast<unsigned>(hash[i]);
        }
        return "0x" + ss.str();
    }

    /**
     * Double SHA256 (used in Bitcoin)
     */
    static std::string doubleHash(const std::string& data) {
        std::string firstHash = hash(data);
        // Remove 0x prefix for second hash
        std::string hashData = firstHash.substr(2);
        return hash(hashData);
    }
};

/**
 * RIPEMD160 Hash (used in Bitcoin addresses)
 */
class RIPEMD160Hash {
public:
    static std::string hash(const std::string& data) {
        unsigned char hash[20];
        EVP_MD_CTX* context = EVP_MD_CTX_new();

        if (context == nullptr) {
            throw std::runtime_error("Failed to create hash context");
        }

        if (EVP_DigestInit_ex(context, EVP_ripemd160(), nullptr) != 1 ||
            EVP_DigestUpdate(context, data.c_str(), data.length()) != 1 ||
            EVP_DigestFinal_ex(context, hash, nullptr) != 1) {
            EVP_MD_CTX_free(context);
            throw std::runtime_error("RIPEMD160 hash failed");
        }

        EVP_MD_CTX_free(context);

        return Keccak256::bytesToHex(hash, 20);
    }
};

// Demo usage
int main() {
    std::cout << "=== Keccak256 Hash Examples ===" << std::endl;

    // Hash a simple message
    std::string message = "Hello, Ethereum!";
    std::string hash = Keccak256::hash(message);
    std::cout << "\nMessage: " << message << std::endl;
    std::cout << "Keccak256: " << hash << std::endl;

    // Compute Ethereum address
    std::string pubKey = "04abcdef1234567890..."; // Example public key
    std::string address = Keccak256::computeAddress(pubKey);
    std::cout << "\nPublic Key: " << pubKey << std::endl;
    std::cout << "Address: " << address << std::endl;

    // SHA256 example
    std::cout << "\n=== SHA256 Hash Examples ===" << std::endl;
    std::string sha256Hash = SHA256Hash::hash(message);
    std::cout << "SHA256: " << sha256Hash << std::endl;

    // Double SHA256 (Bitcoin)
    std::string doubleSha = SHA256Hash::doubleHash(message);
    std::cout << "Double SHA256: " << doubleSha << std::endl;

    // RIPEMD160 example
    std::cout << "\n=== RIPEMD160 Hash Example ===" << std::endl;
    std::string ripemd = RIPEMD160Hash::hash(message);
    std::cout << "RIPEMD160: " << ripemd << std::endl;

    return 0;
}
