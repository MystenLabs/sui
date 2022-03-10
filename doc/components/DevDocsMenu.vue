<template>
  <!-- Docs Side Menu -->
  <section class="widget widget_nav_menu">
    <h3 class="_sui_side_menu_header">Get Started</h3>
    <ul class="menu">
      <li
        v-for="(itm, i) in docsMenu"
        :key="i"
        :class="{ activeList: selectecMenu.topmenu === itm.subMenuTitle }"
        class="_doc-submenu"
      >
        <span
          class="_titleCase _devTopMenu"
          @click="isActive(itm.subMenuTitle)"
          >{{ itm.subMenuTitle }}</span
        >
        <ul class="_subMenu list_details">
          <li
            v-for="(item, id) in itm.menu"
            :key="id"
            :class="{
              '_doc-submenu subList': item.submenu,
              activeList: selectecMenu.sub === item.title,
            }"
            @click="isActive(itm.subMenuTitle, item.title)"
          >
            <nuxt-link
              v-if="item.path && !item.external"
              :to="`${item.path}/`"
              :aria-label="item.title"
              >{{ item.title }}</nuxt-link
            >
            <a
              v-if="item.path && item.external"
              :href="`${item.path}`"
              :aria-label="item.title"
              target="_blank"
              >{{ item.title }}</a
            >
            <span v-if="!item.path" class="_titleCase _devTopMenu _sub-menu">{{
              item.title
            }}</span>

            <ul v-if="item.submenu" class="_subMenu list_details">
              <li v-for="(childList, k) in item.submenu" :key="k">
                <NuxtLink
                  v-if="!childList.external"
                  :to="`${childList.path}/`"
                  :aria-label="childList.title"
                  prefetch
                  >{{ childList.title }}</NuxtLink
                >
                <a
                  v-if="childList.external"
                  :href="`${childList.path}/`"
                  :aria-label="childList.title"
                  >{{ childList.title }}</a
                >
              </li>
            </ul>
          </li>
        </ul>
      </li>
    </ul>
  </section>
  <!-- .nav menu-->
</template>

<script lang="ts">
import { Component, Vue } from 'nuxt-property-decorator'
import { menuOrderGenerator } from '../utils/menuFilter'
import { RootState } from '~/store'

@Component
export default class DevDocsMenu extends Vue {
  routePath: string = this.$route ? this.$route.path.slice(0, -1) : ''
  activeLink: any = menuOrderGenerator('title', this.routePath)
  selectecMenu: any = this.activeLink
    ? {
        topmenu: this.activeLink.categoryName,
        sub: this.activeLink.subCategoryName || '',
      }
    : ''

  get docsMenu(): Array<any> {
    return (this.$store.state as RootState).docMenu
  }

  isActive(path: any, subMenuTitle?: string) {
    this.selectecMenu = { topmenu: path, sub: subMenuTitle || '' }
  }
  mounted() {
    console.log(this.activeLink)
  }
}
</script>

<style lang="scss" scoped>
._doc-submenu {
  cursor: pointer;
  &::before {
    content: '';
    position: absolute;
    display: inline-block;
    border: solid #c4c4c4;
    border-width: 0 2px 2px 0;
    width: 4px;
    height: 4px;
    padding: 1.5px;
    transform: translate(-1rem, -0.4rem) rotate(-45deg);
    -webkit-transform: translate(-1rem, -0.4rem) rotate(-45deg);
    left: 0;
    margin-top: 15px;
    z-index: 1;
    pointer-events: none;
    -webkit-transition: all 0.4s ease;
    -o-transition: all 0.4s ease;
    transition: all 0.4s ease;
  }
  ._sub-menu {
    cursor: pointer;
  }
  &.subList::before {
    left: 12px;
  }
  &.activeList {
    &::before {
      margin-top: 20px;
      right: 0px;
      border-color: #c4c4c4;
    }
  }
}
.list_details {
  opacity: 0;
  max-height: 0;

  overflow: hidden;
  transition: 200ms linear;
  will-change: opacity, max-height;
  li {
    list-style: none;
  }
}

._subMenu {
  li a {
    font-size: 14px;
  }
}
._devTopMenu {
  color: #111111;
}
.activeList {
  > .list_details {
    // transition: all 0.25s ease-in-out;
    opacity: 1;
    max-height: 100%;
    margin-top: 10px;
    transition: all 200ms linear;
    will-change: opacity, max-height;
  }

  &::before {
    transform: translate(-1rem, -0.7rem) rotate(45deg);
    -webkit-transform: translate(-1rem, -0.7rem) rotate(45deg);
  }
}
</style>
