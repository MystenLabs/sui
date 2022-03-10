<template>
  <section class="row">
    <div class="_side-doc-menu col-lg-4">
      <aside class="sidebar widget-area sticky">
        <div class="col-auto">
          <NuxtLink class="logo" to="/" aria-label="Home">
            <div class="logo sui-logo"></div>
          </NuxtLink>
        </div>
        <button
          class="_search button _s_t"
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
          <div class="search-text">Search</div>
        </button>

        <DevDocsMenu class="_sue__spacer" />
        <SideNavPrimary class="_sue__spacer" />
        <ApiReferenceMenu class="_sue__spacer" />
        <!--<TableOfContent class="_sue__spacer" :toc="document.toc" /> -->
        <SideMenuSocial />
      </aside>
    </div>
    <div class="_doc-content col-lg-8">
      <div class="_docBody">
        <div class="section-blog__wrapper-post">
          <article class="post">
            <div class="post__content">
              <nuxt-content :document="document" class="_suiMD" />
            </div>
          </article>
        </div>

        <div class="section-masthead__meta text-left">
          <ul class="post-meta">
            <li>
              <a class="_sui_small-txt" :href="editLink"
                ><i class="fa fab fa-github"></i> Source Code</a
              >
            </li>
          </ul>
        </div>

        <!--
          <div class="divider"></div>
          <div class="section-cta__wrapper-button row justify-content-between p-20">
          <NuxtLink  v-if="prev" class="_docSNav" :to="`${prev.path}/`" >
            <div class="slider__arrow" >keyboard_arrow_left</div>
            <span class="truncate"> {{prev.title }} </span>
          </NuxtLink>
          <NuxtLink v-if="next" class="_docSNav justify-end" :to="`${next.path}/`" >
            <span class="truncate"> {{ next.title }} </span>
            <div class="slider__arrow " >keyboard_arrow_right</div>
          </NuxtLink>
        </div>-->
      </div>
    </div>
  </section>
</template>
<script lang="ts">
import { Component, Vue, Prop } from 'nuxt-property-decorator'

@Component
export default class DocsContent extends Vue {
  @Prop({ required: true }) readonly document!: object
  @Prop({ required: true }) readonly editLink!: string
  @Prop() readonly prev!: object
  @Prop() readonly next!: object

  scrollTo(refName: string) {
    this.$scrollToElment(refName)
  }

  openSearchOverlay() {
    this.$store.commit('setSearchState', true)
    this.$closeMenu()
  }

  closeMenufn(): void {
    return this.$closeMenu()
  }

  mounted() {
    // if Search is active disable searchOverlay
    this.$store.commit('setSearchState', false)

    /// If page hash is present scroll to it
    const hash = this.$route.hash
    if (hash) {
      const $this = this
      setTimeout(() => {
        $this.$scrollToElment(hash.replace('#', ''))
      }, 100)
    }
  }
}
</script>
