<script lang="ts">
    import { createEventDispatcher } from "svelte"
    import { onMount } from "svelte"
    import Loader from "./Loader.svelte"
    import Error from "./Error.svelte"
    import BgOject from "./BgOject.svelte"
    import { walletAddress } from '../store'
    

    //Check if metamask is installed 
    let isweb3Enabled = typeof window.ethereum !== 'undefined'

    //Check if wallect is connected, get connected wallet
    $walletAddress = isweb3Enabled ? window.ethereum.selectedAddress : false

    let isloading: boolean = false
    $:error = false

    const dispatch = createEventDispatcher()

    const switchMTWalletAddress = async () => {
        try {
            await window.ethereum.request({ method: "wallet_requestPermissions", 
                params:[{
                    eth_accounts: {}
                }]
            }).then(()=> connectToWallet())
        } catch (err) {
            error  =  err.message
            isloading = false
        }
    }
    
    /// Select Wallet address to pull token from 
    const selectedAddress = () => {
      dispatch('pageFn', {page:'NFTlist', walletAdress: $walletAddress});
    }

    // connect to metamask
    const connectToWallet = async () => {
        isloading = true;
        if (isweb3Enabled) {
            try {
                const walletAddressList = await window.ethereum.request({ method: 'eth_requestAccounts' });
                $walletAddress = walletAddressList[0];
                isloading = $walletAddress ? false : true;
                return $walletAddress
            } catch (err) {
                error  = err.code === 4001 ? 'Please connect to MetaMask.' : err.message
                isloading = false;
            }
        }
    }
    onMount(async ()=> {
        isweb3Enabled = typeof window.ethereum !== 'undefined'
        $walletAddress = isweb3Enabled ? window.ethereum.selectedAddress : false
    });

    // Handle wallet address change
    window.ethereum.on('accountsChanged', (accounts) => {
        error = false
        $walletAddress  = accounts[0]
    })
</script>
<!-- svelte-ignore non-top-level-reactive-declaration -->
<section class="_sui_landing_page section ">
    <div class="welcome">
        <section class="banner banner-style-2">
            <div class="_m_wallets">
                <div class="container">
                    <div class="row align-items-center">
                        <div class="col-lg-12">
                            <div class="banner-content">
                                <h1 class="title">Sui Mirror </h1>
                                <p>Turpis egestas integer eget aliquet nibh praesent tristique magna sit. Vitae purus faucibus ornare suspendisse. Consectetur adipiscing elit duis tristique sollicitudin. Viverra aliquet eget sit amet tellus cras adipiscing enim. Cursus turpis massa tincidunt dui ut ornare lectus sit amet. Dui sapien eget mi proin sed libero enim sed faucibus. </p>
                                <Loader state={isloading} />
                            
                                {#if $walletAddress}
                                    <h3 class="walletAddress truncate">{$walletAddress}</h3>
                                {/if}
                                {#if isweb3Enabled && !isloading}
                                    {#if $walletAddress}
                                        <button  class="axil-btn btn-fill-white btn-large" on:click="{selectedAddress}">See NFTs</button>
                                    {/if}
                                    <button  class="axil-btn btn-fill-white btn-large" on:click="{$walletAddress ? switchMTWalletAddress : connectToWallet }">{$walletAddress ? 'Change Address' : `Connet Wallet`  } <img class="metamask-logo" src="assets/logos/metamask-fox.svg" alt="metamask" /> </button>
                                {/if}

                                {#if !isweb3Enabled} 
                                    <p class="noWallet"><a href="https://metamask.io/" target="_blank">(MetaMask not enabled) Please install MetaMask</a></p>
                                {/if}
                      
                                <Error errmessage={error} />    
                            </div>
                        </div>
                    </div>
                </div>
                <BgOject />
            </div>
            
        </section>
        <BgOject />
        <ul class="shape-group-7 list-unstyled">
            <li class="shape shape-1"><img src="./../assets/background/circle-2.png" alt="circle"></li>
            <li class="shape shape-2"><img src="./../assets/background/bubble-2.png" alt="Line"></li>
            <li class="shape shape-3"><img src="./../assets/background/bubble-1.png" alt="Line"></li>
        </ul>
    </div>

</section>
<style lang="scss">
@import "../styles/variables.scss"; 
._sui_landing_page {
    background-color: $sui__white;
    min-height: 90vh;
    text-align: center;
    display: flex;
    flex-direction: column;
    justify-content: center;
    h1{
        margin-top: 0;
    }
    .axil-btn {
        margin-bottom: 10px;
        min-width: 300px;
        @media only screen and (max-width: 767px) {
            width: 100%;
        }
    }
    .welcome{
        display: flex;
        flex-direction: column;
        justify-content: center;
        height: 89vh;
    }

    .banner-content p{
        max-width: 700px;
        margin: 0 auto;
        margin-bottom: 20px;
        text-align: left;
        font-weight: 400;
        font-size: 18px;
        
    }

    .shape-1{
        top: -90px;
        right: -55%;
    }
    

    .banner {
        max-width: 1200px;
        width: 100%;
        border-radius: 20px;
        margin: auto;
        display: flex;
        justify-content: center;
        align-items: center;
        @media only screen and (max-width: 767px) {
            width: 91%;
            padding: 20px;
        }
    }
  }

 .walletAddress{
    font-size: 1.2rem;
    font-weight: 600;
    margin-bottom: 20px;
    text-align: center;
    margin: auto;
    margin-bottom: 20px;
    transition: all 0.3s cubic-bezier(0.785, 0.135, 0.15, 0.86);
    transition-delay: 100ms;
 }
 .noWallet{
     text-align: center !important;
   
 }

 .shape-group-7 .shape.shape-2{
    top: 98px;
    left: 52%;
 }
</style>