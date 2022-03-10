import { mount, createLocalVue } from '@vue/test-utils'
import Vuex from 'vuex'
import SearchOverlay from '@/components/SearchOverlay.vue'

const localVue = createLocalVue()
localVue.use(Vuex)

let wrapper

beforeEach(() => {
  wrapper = mount(SearchOverlay, {
    store: new Vuex.Store({
      state: { search: [] },
      getters: { setSearchState: () => () => {} },
    }),
    stubs: {
      NuxtLink: true,
      // Any other component that you want stubbed
    },
    localVue,
  })
})

afterEach(() => {
  wrapper.destroy()
})

describe('SearchOverlay', () => {
  test('is a Vue instance', () => {
    // const wrapper = mount(Header)
    expect(wrapper.vm).toBeTruthy()
  })
})
