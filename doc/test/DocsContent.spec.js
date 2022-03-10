import { shallowMount, createLocalVue, config } from '@vue/test-utils'
import Vuex from 'vuex'
import VueRouter from 'vue-router'
import DocsContent from '@/components/DocsContent.vue'

const localVue = createLocalVue()
localVue.use(Vuex)
localVue.use(VueRouter)

const router = new VueRouter({ mode: 'abstract' })

describe('DocsContent Props', () => {
  let wrapper
  beforeEach(() => {
    wrapper = shallowMount(DocsContent, {
      mocks: config,
      propsData: {
        editLink: 'markdown url link',
        document: {
          toc: [],
        },
        prev: {},
        next: {},
      },
      store: new Vuex.Store({
        mutations: { setSearchState: () => () => false },
      }),
      stubs: {
        NuxtLink: true,
        DevDocsMenu: true,
        NuxtContent: true,
        TableOfContent: true,
        SideNavPrimary: true,
        ApiReferenceMenu: true,
        SideMenuSocial: true
      },
      localVue,
      router,
    })
  })

  it('checks the prop editLink ', () => {
    expect(wrapper.props().editLink).toBe('markdown url link')
  })

  test('is a Vue instance', () => {
    expect(wrapper.vm).toBeTruthy()
  })
})
