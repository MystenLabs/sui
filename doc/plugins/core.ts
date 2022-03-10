import Vue from 'vue'
import { baseFn, closeMobleMenu, scrollToElment } from './coreFE'

declare module 'vue/types/vue' {
  interface Vue {
    $coreDemoFunction(): void
    $closeMenu(): void
    $scrollToElment(elName: string): void
  }
}

 Vue.prototype.$coreDemoFunction = () => baseFn()

Vue.prototype.$closeMenu = () => closeMobleMenu()

Vue.prototype.$scrollToElment = (elName: string) => scrollToElment(elName)
