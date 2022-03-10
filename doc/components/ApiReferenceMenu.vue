<template>
  <!-- Docs Side Menu -->
  <section class="widget widget_nav_menu">
    <h3 class="_sui_side_menu_header">API <span class="_api-version"><span class="_version">v</span> {{apiReferenceMenu.info}}</span></h3>

    <ul class="menu">
      <li
        v-for="(itm, i) in apiReferenceMenu.menu"
        :key="i"
        :class="{ activeList: selectecMenu.topmenu === itm.name }"
        class="_doc-submenu"
      >
        <span
          class="_titleCase _devTopMenu"
          @click="isActive(itm.name)"
          >{{ itm.name }}</span
        >
        <ul class="_subMenu list_details">
          <li
            v-for="(item, id) in itm.subMenu"
            :key="id"
            :class="{
              '_doc-submenu1 subList': item.name,
              activeList: selectecMenu.sub === item.name,
            }"
            @click="isActive(itm.name, item.name)"
          >

            <a
              :href="`${item.link}`"
              :aria-label="item.name"
              target="_blank"
              >{{ item.name }}</a
            >

          </li>
        </ul>
      </li>
    </ul>
  </section>
  <!-- .nav menu-->
</template>

<script lang="ts">
import { Component, Vue } from 'nuxt-property-decorator'
import { RootState } from '~/store'

@Component
export default class ApiReferenceMenuvue extends Vue {
  selectecMenu: any = {
        topmenu: '',
        sub: '',
    }


  get apiReferenceMenu(): Array<any> {
    return (this.$store.state as RootState).apiReference
  }

  isActive(path: any, subMenuTitle?: string) {
    this.selectecMenu = { topmenu: path, sub: subMenuTitle || '' }
  }
  mounted() {

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
    font-weight: 600;
    line-height: 16px;
    color: #111111;
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
._titleCase{
  font-weight: 600;
}
._api-version{
  font-weight: 400;
  font-size: 14px;
}

</style>
