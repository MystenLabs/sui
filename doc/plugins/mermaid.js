import Vue from 'vue'

const mermaid = require('mermaid/dist/mermaid')

const plugin = {
  install() {
    Vue.mermaid = mermaid
    Vue.prototype.$mermaid = mermaid
  },
}

if (process.client) {
  window.onSuiReady(() => {
    mermaid.initialize({ startOnLoad: true })
    mermaid.init()
  })
}

Vue.use(plugin)
