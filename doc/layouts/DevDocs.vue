<template>
  <div class="page-wrapper sui-light">
    <SearchOverlay v-if="seachState" />
    <!--<Header class="header_dark" :showsearchbtn="true" />-->
    <nuxt class="site-main _dev_docs section_doc" />
  </div>
</template>

<script lang="ts">
import { Vue, Component } from 'vue-property-decorator'
import { MetaInfo } from 'vue-meta'

@Component({
  layout: 'DevDocs',
})
export default class DevDocs extends Vue {
  // Get search state from store
  protected get seachState(): boolean {
    return this.$store.getters.getSearchState
  }

  head(): MetaInfo {
    return {
      bodyAttrs: {
        class: '_sui-dev-docs',
      },
    }
  }
}
</script>
