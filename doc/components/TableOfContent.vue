<template>
  <section class="_toc">
    <section
      v-if="toc.length"
      class="widget widget_recent_entries w_light _fixeds sticky _t-content"
    >
      <client-only>
        <scrollactive active-class="__active" :offset="280">
          <h3 class="_sui_side_menu_header">Table of Content</h3>
          <ul>
            <li
              v-for="link of toc"
              :key="link.id"
              class="_light _in-page-menu"
              :class="{ toc2: link.depth === 2, toc3: link.depth === 3 }"
            >
              <NuxtLink
                :to="`#${link.id}`"
                class="scrollactive-item"
                @click.native="scrollTo(link.id)"
                >{{ link.text }}</NuxtLink
              >
            </li>
          </ul>
        </scrollactive>
      </client-only>
    </section>
  </section>
</template>

<script lang="ts">
import { Component, Vue, Prop } from 'nuxt-property-decorator'

@Component
export default class TableOfContent extends Vue {
  @Prop({ required: true }) readonly toc!: Array<any>

  scrollTo(refName: string) {
    this.$scrollToElment(refName)
  }
}
</script>
<style lang="scss"></style>
