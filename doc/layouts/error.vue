<template>
  <section class="_sui-err">
    <!-- section 404 -->
    <div
      class="_sui-err__404 section section-masthead section_pt section_pb sui-blue"
    >
      <section class="section section-fullheight section-404 bg-light">
        <div class="section-fullheight__inner section-404__inner">
          <div class="container">
            <div class="row justify-content-center align-items-center">
              <div class="col-lg-6">
                <div class="section-404__content">
                  <h1>
                    {{ error.statusCode === 404 ? pageNotFound : otherError }}
                  </h1>
                  <div class="section-404__text">
                    <div class="section-404__headline"></div>
                    <p>
                      It looks like nothing found at this location. Try to
                      navigate the menu or go to the home page.
                    </p>
                  </div>
                  <div class="section-404__wrapper-button">
                    <nuxt-link class="button button_accent" to="/"
                      ><span>Go to Homepage</span></nuxt-link
                    >
                  </div>
                </div>
                <div class="section-404__square"></div>
                <div class="section-404__box"></div>
                <div class="section-404__big">404</div>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>

    <!-- - section 404 -->
  </section>
</template>
<script lang="ts">
import { Component, Vue, Prop } from 'nuxt-property-decorator'
interface ErrorResponse {
  readonly statusCode: number
}
@Component({
  layout: 'empty',
  head(this: Error): {} {
    return {
      title:
        this.error.statusCode === 404 ? this.pageNotFound : this.otherError,
    }
  },
})
export default class Error extends Vue {
  // props
  @Prop({ default: null })
  error!: ErrorResponse

  // declare variables
  pageNotFound: string = "That page can't be found"
  otherError: string = 'An error occurred'
}
</script>
<style lang="scss" scoped>
._sui-err {
  .section {
    margin: 0;
  }
  p,
  h1 {
    color: #161b25;
  }
}
</style>
