cmake_minimum_required(VERSION 3.10)
project(CryptoHashes)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Find OpenSSL
find_package(OpenSSL REQUIRED)

# Add executable
add_executable(keccak256 keccak256.cpp)

# Link OpenSSL
target_link_libraries(keccak256 OpenSSL::Crypto)

# Compiler warnings
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(keccak256 PRIVATE -Wall -Wextra -Wpedantic)
endif()
