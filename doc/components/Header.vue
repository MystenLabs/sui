<template>
  <!-- PAGE HEADER -->
  <header class="header site-header js-header-sticky sui_menu_inactive">
    <div class="container-fluid">
      <div class="row justify-content-between align-items-center">
        <div class="col-auto">
          <NuxtLink class="logo" to="/" aria-label="Home">
            <div class="logo sui-logo"></div>
          </NuxtLink>
        </div>
        <div class="col-auto d-xl-block d-none">
          <ul class="menu">
            <li
              v-for="(itm, i) in navmenu"
              :key="i"
              :class="{ 'social__item ': itm.external }"
            >
              <nuxt-link v-if="!itm.external" :to="itm.link" prefetch>{{
                itm.title
              }}</nuxt-link>
              <a
                v-else
                :href="itm.link"
                :title="itm.title"
                :class="`fa fab fa-${itm.title.toLowerCase()}`"
                target="_blank"
              >
                <i :class="`fa fa-${itm.title.toLowerCase()}`"></i>
              </a>
            </li>
            <!-- show markdown pages link-->
            <li>
              <div v-if="showsearchbtn">
                <button
                  class="_search button"
                  type="button"
                  autocomplete="off"
                  @click="openSearchOverlay"
                >
                  <div class="search_icon">
                    <svg
                      viewBox="0 0 20 20"
                      fill="currentColor"
                      class="h-full py-2 text-grey-800 opacity-75 fill-current group-hover:animate-wiggle-once group-focus:animate-wiggle-once"
                    >
                      <path
                        d="M19.71,18.29,16,14.61A9,9,0,1,0,14.61,16l3.68,3.68a1,1,0,0,0,1.42,0A1,1,0,0,0,19.71,18.29ZM2,9a7,7,0,1,1,12,4.93h0s0,0,0,0A7,7,0,0,1,2,9Z"
                      ></path>
                    </svg>
                  </div>
                  <div class="">Search the documentation</div>
                </button>
              </div>
            </li>
          </ul>
        </div>

        <div class="col-auto d-xl-none">
          <div class="burger js-burger">
            <div class="burger__line"></div>
            <div class="burger__line"></div>
            <div class="burger__line"></div>
          </div>
        </div>
      </div>
    </div>

    <div class="header__wrapper-overlay-menu d-xl-none">
      <div class="container">
        <div class="row justify-content-center">
          <div class="col-auto _sui-menu">
            <ul class="overlay-menu js-overlay-menu">
              <li v-for="(item, k) in navmenu" :key="k">
                <nuxt-link
                  v-if="!item.external"
                  :to="item.link"
                  @click.native="closeMenufn"
                >
                  <div class="overlay-menu__item-wrapper">
                    <div class="overlay-menu__item-line"></div>
                    <div class="overlay-menu__item-counter">0{{ k + 1 }}</div>
                    <span>{{ item.title }}</span>
                  </div></nuxt-link
                >

                <a v-else :href="item.link" :title="item.title" target="_blank">
                  <div class="overlay-menu__item-wrapper">
                    <div class="overlay-menu__item-line"></div>
                    <div class="overlay-menu__item-counter">0{{ k + 1 }}</div>
                    <span>{{ item.title }}</span>
                  </div>
                </a>
              </li>
              <!-- demo Menu show markdown pages link-->
              <li class="_search_btn-item">
                <div v-if="showsearchbtn">
                  <button
                    class="_search button"
                    type="button"
                    autocomplete="off"
                    @click="openSearchOverlay"
                  >
                    <div class="search_icon">
                      <svg
                        viewBox="0 0 20 20"
                        fill="currentColor"
                        class="h-full py-2 text-grey-800 opacity-75 fill-current group-hover:animate-wiggle-once group-focus:animate-wiggle-once"
                      >
                        <path
                          d="M19.71,18.29,16,14.61A9,9,0,1,0,14.61,16l3.68,3.68a1,1,0,0,0,1.42,0A1,1,0,0,0,19.71,18.29ZM2,9a7,7,0,1,1,12,4.93h0s0,0,0,0A7,7,0,0,1,2,9Z"
                        ></path>
                      </svg>
                    </div>
                    <div class="">Search the documentation</div>
                  </button>
                </div>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  </header>
  <!-- - PAGE HEADER -->
</template>

<script lang="ts">
import { Component, Vue, Prop } from 'nuxt-property-decorator'
import { RootState } from '~/store'

@Component
export default class Header extends Vue {
  @Prop({ required: false, default: true }) readonly showsearchbtn!: boolean

  get navmenu(): Array<any> {
    return (this.$store.state as RootState).menu
  }

  openSearchOverlay() {
    this.$store.commit('setSearchState', true)
    this.$closeMenu()
  }

  closeMenufn(): void {
    return this.$closeMenu()
  }

  mounted(): any {
    const $this = this
    //  $this.$coreDemoFunction()
    setTimeout((): void => {
        $this.$coreDemoFunction()
    }, 350)
  }
}
</script>
<style lang="scss" scoped>
.header_light.header_sticky {
  background-color: #fefefe;
  box-shadow: 0 2px 20px rgba(0, 0, 0, 0.08) ;
  .menu > li > a {
    color: #000;
  }
}

._sui-menu {
  span {
    text-transform: capitalize;
  }
  @media (max-width: 960px) {
    width: 100%;
  }
}
._sui__light {
  ._sui-menu {
    span {
      color: #fff;
    }
  }
}
.header ._search {
  color: #111111;
  text-align: center;
}
.header_dark {
  &.header_sticky {
      box-shadow: 0 2px 20px rgba(0, 0, 0, 0.08) ;
  }
  .burger__line{
    background-color: #111111;
  }
}
</style>
