import { shallowMount, createLocalVue } from '@vue/test-utils'
import Vuex from 'vuex'
import VueMeta from 'vue-meta'
import HomePage from '@/pages/index.vue'

const localVue = createLocalVue()
localVue.use(VueMeta, { keyName: 'head' })
localVue.use(Vuex)
let wrapper

beforeEach(() => {
  wrapper = shallowMount(HomePage, {
    head: {
      title: 'Sui Developer Hub',
    },
    store: new Vuex.Store({
      state: { homeData: {} },
    }),
    stubs: {
      NuxtLink: true,
    },
    localVue,
  })
})

afterEach(() => {
  wrapper.destroy()
})

describe('HomePage', () => {
  test('is a Vue instance', () => {
    expect(wrapper.vm).toBeTruthy()
  })
})
