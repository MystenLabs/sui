import { mount, createLocalVue } from '@vue/test-utils'
import Vuex from 'vuex'
import Header from '@/components/Header.vue'

const localVue = createLocalVue()
localVue.use(Vuex)

let wrapper

beforeEach(() => {
  const $closeMenu = {
    start: jest.fn(),
    finish: () => {},
  }
  const $coreDemoFunction = {
    start: jest.fn(),
    finish: () => {},
  }
  wrapper = mount(Header, {
    store: new Vuex.Store({
      state: { menu: [], search: Boolean },
      mutations: { setMenuState: () => () => {} },
    }),
    stubs: {
      NuxtLink: true,
      // Any other component that you want stubbed
    },
    mocks: { $closeMenu, $coreDemoFunction },

    localVue,
  })
})

afterEach(() => {
  wrapper.destroy()
})

describe('Header', () => {
  test('is a Vue instance', () => {
    // const wrapper = mount(Header)
    expect(wrapper.vm).toBeTruthy()
  })
})
