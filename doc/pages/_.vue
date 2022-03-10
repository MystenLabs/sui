<template>
  <DocsContent
    :document="document"
    :edit-link="editLink"
    :prev="prev"
    :next="next"
  />
</template>

<script lang="ts">
import { Vue, Component } from 'nuxt-property-decorator'
import { MetaInfo } from 'vue-meta'

@Component({
  layout: 'DevDocs',

  /// Dynamic pull markdown file based on Params slug regardless of the directory depth
  // asyncData hook (page context), server rendering
  async asyncData({ $content, params, error }: any): Promise<any> {
    try {
      // concat slug, join with '/' and split last / if there is
      const editUrl =
        '/' +
        Object.keys(params)
          .map((key) => params[key])
          .join('/')
          .replace(/\/$/, '')
      /// Fetch page content from markdown file based on Params slug
      const page: any = await $content(editUrl).fetch()

      /// Get Previous and Next page Order by categoryOrder and itemOder
      const [prev, next]: any = await $content({ deep: true })
        .only(['title', 'slug', 'path', 'categoryOrder', 'itemOder'])
        /// Sort
        .sortBy('categoryOrder', 'asc')
        .sortBy('itemOder', 'asc')
        .surround(editUrl)
        .fetch()

      return {
        title: page.title,
        page_meta: {},
        editLink: `https://github.com/MystenLabs/fastx_dev_portal/tree/content/content/docs/${editUrl}.md`,
        document: page,
        prev,
        next,
      }
    } catch (err: any) {
      return error({ statusCode: 404, message: 'Page Not Found' })
    }
  },
})
export default class DevDocsPageComponent extends Vue {
  title!: string
  page_meta: any
  editLink!: string
  prev!: any
  next!: any

  head(): MetaInfo {
    return {
      title: `Sui- ${this.title || 'Docs'}`,
    }
  }
}
</script>
