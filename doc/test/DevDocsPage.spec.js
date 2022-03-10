import { shallowMount, createLocalVue } from '@vue/test-utils'
import VueMeta from 'vue-meta'
import DevDocsPageComponent from '@/pages/_.vue'

const $route = {
  path: '/move/test/',
  params: {
    move: 'Docs',
  },
}

const localVue = createLocalVue()
localVue.use(VueMeta, { keyName: 'head' })

describe('DevDocsPageComponent', () => {
  let wrapper
  beforeEach(() => {
    wrapper = shallowMount(DevDocsPageComponent, {
      mocks: { $route },
      stubs: {
        DocsContent: true,
        NuxtLink: true,
      },
      localVue,
    })
  })
  const title = 'Sui- Docs'

  test('has its meta title correctly set', () => {
    expect(wrapper.vm.$meta().refresh().metaInfo.title).toBe(title)
  })

  test('is a Vue instance', () => {
    expect(wrapper.vm).toBeTruthy()
  })
})
