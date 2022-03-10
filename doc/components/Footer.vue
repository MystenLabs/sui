<template>
  <!-- PAGE FOOTER -->
  <footer class="footer site-footer">
    <div class="container footermenu">
      <div class="footer__divider"></div>
      <div class="row footer-menu">
        <div
          v-for="(itm, i) in footerData.menu"
          :key="i"
          class="col-lg-4 col-md-4 col-6"
        >
          <ul class="menu _no_margin">
            <h4>{{ itm.title }}</h4>
            <li v-for="(sub, k) in itm.listItems" :key="k" class="menu-item">
              <nuxt-link v-if="!sub.external" :to="sub.link">{{
                sub.title
              }}</nuxt-link>
              <a v-else :href="sub.link" :title="sub.title" target="_blank">
                {{ sub.title }}
              </a>
            </li>
          </ul>
        </div>
      </div>
    </div>
    <div class="container-fluid">
      <div class="footer__inner">
        <div class="row align-items-center">
          <div class="col-lg-4 text-center footer__column order-lg-2">
            <nuxt-link class="logo" to="/">
              <div class="logo _sui-footer-logo"></div>
            </nuxt-link>
            <p>{{ footerData.copyRight }}</p>
          </div>
          <div class="col-lg-4 text-right footer__column order-lg-3"></div>
          <div class="col-lg-4 text-left footer__column order-lg-1">
            <small></small>
          </div>
        </div>
      </div>
    </div>
  </footer>
  <!-- - PAGE FOOTER -->
</template>

<script lang="ts">
import { Component, Vue } from 'nuxt-property-decorator'
import { RootState } from '~/store'

@Component
export default class BaseFooter extends Vue {
  get footerData(): Array<any> {
    return (this.$store.state as RootState).footer
  }
}
</script>
<style lang="scss" scoped>
.footer-menu {
  list-style: none;
  margin: 0;
  padding: 40px 0;
  text-align: left;
  .menu {
    display: inline-grid;
    > li a {
      color: #111111;
      font-size: 15px;
      line-height: 150%;
      padding: 5px 0px 15px;
      display: inline-block;
      &:hover {
        color: #6fbcf0;
        &:before {
          left: 0;
        }
      }
    }
  }
  ._selectActive {
    color: #6fbcf0 !important;
  }
}
.site-footer {
  background-color: #fefefe;
  ._sui-footer-logo {
    background: url('/img/Sui-logo-dark.png');
    background-size: contain;
    background-repeat: no-repeat;
    background-position: left center;
    height: 32px;
    width: 100px;
  }
}
._sui__light {
  .site-footer {
    background-color: #6fbcf0;
    color: #fefefe;
  }
  ._sui-footer-logo {
    height: 32px;
    background: url('/img/Sui-logo-light.png');
    background-size: contain;
    background-repeat: no-repeat;
    background-position: left center;
    height: 32px;
    width: 100px;
  }
  .footer-menu {
    .menu {
      h4 {
        font-weight: 600;
      }
      > li a {
        color: #ffffff;
        &:hover {
          color: #ffffff;
        }
      }
    }
  }
}
</style>
