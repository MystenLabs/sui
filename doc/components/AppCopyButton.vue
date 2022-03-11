<template>
  <div ref="copy" class="copy">
    <div class="post__tags">
    <div class="tagcloud">
      <ul ref="copy" >
        <li v-if="state === 'copied'">
          <a class="tag-cloud-link" href="javascript:void(0)">Copied to clipboard</a>
        </li>
        <li v-else><a class="tag-cloud-link" href="javascript:void(0)">Copy</a></li>
      </ul>
    </div>
  </div>
  </div>

</template>

<script lang="ts">
import { Vue, Component } from 'vue-property-decorator'
import Clipboard from 'clipboard'

@Component
export default class AppCopyButton extends Vue {
  state: string = 'init'

  mounted() {

    const ref = this.$refs.copy
     // @ts-ignore
    const copyCode = new Clipboard(ref, {
      target(trigger) {
        return trigger.previousElementSibling
      },
    })
    copyCode.on('success', (event) => {
      event.clearSelection()
      this.state = 'copied'
      window.setTimeout(() => {
        this.state = 'init'
      }, 2000)
    })
  }
}
</script>
<style lang="scss" scoped>
.post__content {
  li:before {
    display: none;
  }
  ul {
    list-style: none;
  }
  .tag-cloud-link{
    color: #6fbcf0;
    background-color:rgba(255, 255, 255, 0.65);
    border: none;
    border: none;
    position: absolute;
    margin-top: -84px;
    right: 10px;

  }
}
</style>
