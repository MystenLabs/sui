<script lang="ts">
  import { walletAddress } from "../store";
  import { createEventDispatcher } from "svelte";
  const dispatch = createEventDispatcher();
  const changewalletAddress = () => {
    dispatch("changeWallet");
  };
</script>

<header class="header sui-header header-style-3">
  <div id="sui-sticky-placeholder" />
  <div class="sui-mainmenu">
    <div class="container">
      <div class="header-navbar">
        <div class="header-logo">
          <img
            class="light-version-logo"
            src="./../assets/Sui-logo-light.png"
            alt="logo"
          />
          <img
            class="dark-version-logo"
            src="./../assets/Sui-logo-blue.png"
            alt="logo"
          />
          <img
            class="sticky-logo"
            src="./../assets/Sui-logo-blue.png"
            alt="logo"
          />
        </div>
        <div class="header-action">
          {#if $walletAddress}
            <ul class="list-unstyled">
              <li class="sidemenu-btn d-lg-block ">
                <button
                  class="sui-btn btn-fill-white btn-large _changeAddress"
                  on:click={changewalletAddress}
                  >Change Address<img
                    class="metamask-logo"
                    src="assets/logos/metamask-fox.svg"
                    alt="metamask"
                  />
                </button>
              </li>
            </ul>
          {/if}
        </div>
      </div>
    </div>
  </div>
</header>

<style lang="scss">
  @import "../styles/variables.scss";
  .header {
    background-color: $sui__blue;
    .header-logo img {
      width: auto;
    }
    ._changeAddress {
      min-width: 300px;
      padding: 8px 45px;
      &:hover {
        color: #000;
      }
      @media only screen and (max-width: 767px) {
        min-width: 100%;
        padding: 5px 15px;
      }
    }
  }
</style>
