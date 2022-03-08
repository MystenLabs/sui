import { writable } from 'svelte/store';

export const ApiConfig:any = writable({});

export const config = {
    url: 'http://localhost:8000/',
    demo: true
}
// 
ApiConfig.set({
    url: 'http://localhost:8000/',
  
  
})

export const walletAddress = writable(false);