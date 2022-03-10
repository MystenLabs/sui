<template>
  <section class="_sui-global-menu">
    <section class="widget widget_recent_entries w_light _fixeds sticky">
      <h3 class="_sui_side_menu_header">{{ primarySideNav.title }}</h3>
      <ul>
        <li
          v-for="(itm, i) in primarySideNav.items"
          :key="i"
          class="_light _in-page-menu"
        >
          <a :href="itm.link">{{ itm.label }}</a>
        </li>
      </ul>
    </section>
  </section>
</template>

<script lang="ts">
import { Component, Vue } from 'nuxt-property-decorator'
import { RootState } from '~/store'
@Component
export default class SideNavPrimary extends Vue {
  get primarySideNav(): any {
    return (this.$store.state as RootState).sideMenu
  }
}
</script>
<style lang="scss">
._sui-global-menu {
  display: inline-block;
  ul {
    margin: 0;
    padding: 0;
    li {
      margin: 10px 0;
      a {
        font-weight: 400;
      }
    }
  }
}
</style>
