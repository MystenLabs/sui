<script lang="ts">
  import LandingPage from "./Components/LandingPage.svelte";
  import ListNFT from "././Components/ListNFT.svelte";
  import Header from "./Components/HeaderComp.svelte";

  $: pageState = {
    page: "landing",
    walletAdress: "",
  };

  const changePageState = (data: any) => {
    pageState.page = data.detail.page;
    // pageState.walletAdress = data.detail.walletAdress;
  };

  let changeWalletAddres;
  const changeWalletAdress = (data: any) => {
    return changeWalletAddres.changeWallet();
  };
</script>

<main class="_sui_main main-wrapper">
  <Header on:changeWallet={changeWalletAdress} />

  {#if pageState.page === "landing"}
    <LandingPage
      on:pageFn={changePageState}
      bind:changeWalleFn={changeWalletAddres}
    />
  {/if}

  {#if pageState.page === "NFTlist"}
    <ListNFT on:pageFn={changePageState} />
  {/if}
</main>

<style type="text/scss" lang="scss" global>
  @import "./styles/variables.scss";
  .sui-logo {
    height: auto;
    width: 100px;
    margin: 0 auto;
    margin-top: 50px;
    margin-bottom: 0px;
    display: block;
  }
</style>
