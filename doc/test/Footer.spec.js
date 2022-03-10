import { mount, createLocalVue, shallowMount } from '@vue/test-utils'
import Vuex from 'vuex'
import BaseFooter from '@/components/Footer.vue'

const localVue = createLocalVue()
localVue.use(Vuex)

let wrapper

beforeEach(() => {
  wrapper = mount(BaseFooter, {
    store: new Vuex.Store({
      state: { footer: [] },
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
describe('BaseFooter', () => {
  test('is a Vue instance', () => {
    // const wrapper = mount(BaseFooter)
    expect(wrapper.vm).toBeTruthy()
  })
})
