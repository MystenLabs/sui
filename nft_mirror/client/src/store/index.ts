import { writable } from 'svelte/store';

/// General configuration
/// whever demo is true, mock data is used for NFTs request and response and for the NFT mirror
/// Mock API response for NFTs mirror will return succeess or error at random
export const config = {
    url: 'http://localhost:8000/',
    demo: true
}

export const walletAddress = writable(false);