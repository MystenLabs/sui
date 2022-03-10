<template>
  <div class="_sui-search-overlay _sui-search-active">
    <header class="header header_fixed site-header">
      <div class="container-fluid">
        <div class="row justify-content-end">
          <div class="col-auto">
            <a class="button-close" href="#" @click="closeSearchOverlay">
              <div class="button-close__line"></div>
              <div class="button-close__line"></div>
            </a>
          </div>
        </div>
      </div>
    </header>
    <section class="section section_mt section_mb-small searchable">
      <div class="container-fluid">
        <div class="row">
          <input
            v-model="query"
            class="_search"
            placeholder="Search the documentation"
            type="search"
            autocomplete="off"
          />
          <div class="_search__result">
            <ul v-if="articles.length">
              <li
                v-for="article of articles"
                :key="article.slug"
                class="_list-items-result"
              >
                <NuxtLink
                  :to="article.path + '/'"
                  @click.native="closeSearchOverlay()"
                  >{{ article.title }}</NuxtLink
                >
              </li>
            </ul>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<script lang="ts">
import { Vue, Component, Watch } from 'vue-property-decorator'

@Component
export default class Search extends Vue {
  query: string = ''
  articles: any = []

  @Watch('query')
  async queryFn(query: any): Promise<void> {
    if (!query) {
      this.articles = []
      return
    }

    this.articles = await this.$content({ deep: true })
      .only(['title', 'description', 'slug', 'text', 'path'])
      .sortBy('createdAt', 'asc')
      .limit(12)
      .search(query)
      .fetch()
  }

  closeSearchOverlay() {
    this.$store.commit('setSearchState', false)
  }
}
</script>

<style lang="scss">
._sui-search-overlay {
  position: fixed;
  z-index: 10;
  left: 0;
  right: 0;
  top: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.6);
  // background-color: rgba(26, 32, 44, 0.56);
  opacity: 0;
  transition: opacity 0.18s;
  visibility: hidden;
  &:before {
    content: '';
    position: absolute;
    left: 50%;
    width: 100vw;
    height: 100%;
    margin-left: -50vw;
    background-color: inherit;
  }
  &._sui-search-active {
    display: block;
    opacity: 1;
    z-index: 502;
    visibility: visible;
    backdrop-filter: blur(12px);

    .button-close__line {
      background-color: #fefefe;
    }
  }
  .searchable {
    margin: 15vh auto;
    max-width: 1200px;
    @media (max-width: 480px) {
      width: 95%;
    }
    ._search {
      height: 80px;
      padding: 0 40px 0 0;
      width: 100%;
      border: 0;
      font-size: 38px;
      font-weight: 300;
      line-height: 1.5;
      margin-top: 36px;
      padding: 0 20px;
      @media (max-width: 480px) {
        font-size: 24px;
        height: 60px;
        padding: 0 10px;
      }
    }
    ._search__result {
      border-bottom: 4px solid#E1F3FF;
      border-top: 4px solid #e1f3ff;
      display: inline-table;
      max-height: 0;
      min-height: 12px;
      overflow: hidden;
      width: 100%;
      transition: max-height 0.54s ease-in-out;
      ul {
        padding: 20px 10px;
        li {
          display: block;
          height: 50px;
          padding: 10px;
          border-bottom: 1px solid rgba(182, 140, 112, 0.2);
          a {
            color: #ffffff;
          }
        }
        ._list-items-result {
          a:hover {
            color: #6fbcf0;
          }
        }
      }
    }
  }
}
</style>
