import { mount, createLocalVue } from '@vue/test-utils'
import Vuex from 'vuex'
import VueRouter from 'vue-router'
import DevDocsMenu from '@/components/DevDocsMenu.vue'

const localVue = createLocalVue()
localVue.use(Vuex)
localVue.use(VueRouter)
const router = new VueRouter()
let wrapper

beforeEach(() => {
  wrapper = mount(DevDocsMenu, {
    store: new Vuex.Store({
      state: { docMenu: [] },
    }),
    stubs: {
      NuxtLink: true,
      // Any other component that you want stubbed
    },
    data() {
      return {
        selectecMenu: '',
      }
    },
    localVue,
    router,
  })
})

afterEach(() => {
  wrapper.destroy()
})

describe('DevDocsMenu', () => {
  test('is a Vue instance', () => {
    // const wrapper = mount(Header)
    expect(wrapper.vm).toBeTruthy()
  })
})
