<template>
  <div class="page-wrapper _sui__light">
    <SearchOverlay v-if="seachState" />
    <Header class="header_light" :showsearchbtn="true" />
    <main class="site-main _sui_light-blue">
      <nuxt />
    </main>
    <Footer />
  </div>
</template>

<script lang="ts">
import { Vue, Component } from 'vue-property-decorator'

@Component
export default class DevDocs extends Vue {
  // Get search state from store
  protected get seachState(): boolean {
    return this.$store.getters.getSearchState
  }
}
</script>
