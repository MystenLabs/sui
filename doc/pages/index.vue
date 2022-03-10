<template>
  <!-- All content on this page are place holder content -->
  <section>
    <!-- Page INTRO -->
    <section
      class="section section-masthead section_pt section_pb bg-gray1 sui-blue"
    >
      <div class="container">
        <div class="row">
          <div class="col">
            <h1>{{ homePageData.title }}</h1>
            <p>{{ homePageData.subTitle }}</p>
          </div>
        </div>
      </div>
      <div class="section-masthead__line"></div>
    </section>

    <!-- - section INTRO -->
    <section class="section section_pd-50">
      <div class="container">
        <div class="justify-content-between align-items-center _sui-blocks">
          <div
            v-for="(itm, i) in homePageData.cta"
            :key="i"
            div
            class="_sui-blocks-item"
          >
            <div>
              <h2>{{ itm.name }}</h2>
              <ul>
                <li v-for="(sub, k) in itm.sublist" :key="k">
                  <a :href="sub.url">{{ sub.name }}</a>
                </li>
              </ul>
            </div>
          </div>
        </div>
      </div>
    </section>
  </section>
</template>

<script lang="ts">
import { Vue, Component } from 'nuxt-property-decorator'
import { MetaInfo } from 'vue-meta'
import { RootState } from '~/store'

@Component
export default class HomePageComponent extends Vue {
  title!: string

  get homePageData(): Array<any> {
    return (this.$store.state as RootState).homeData
  }

  head(): MetaInfo {
    return {
      title: 'Sui Developer Hub',
    }
  }
}
</script>
