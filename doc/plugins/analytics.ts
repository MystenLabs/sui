import Vue from 'vue'
import { initializeApp } from 'firebase/app'
import { getAnalytics, logEvent } from 'firebase/analytics'
import { getPerformance } from 'firebase/performance'

const firebaseConfig = {
  apiKey: 'AIzaSyB1FfOA1KMvN8ka26AHdAn1gFvioNFj818',
  authDomain: 'devportal-30dd0.firebaseapp.com',
  projectId: 'devportal-30dd0',
  storageBucket: 'devportal-30dd0.appspot.com',
  messagingSenderId: '957639685165',
  appId: '1:957639685165:web:acc516d914e224457974a4',
  measurementId: 'G-0GW4F97GFL',
}

const fbapp = initializeApp(firebaseConfig)
const analytics = getAnalytics(fbapp)
declare module 'vue/types/vue' {
  interface Vue {
    $fbapp(): void
    $analytics(): void
    $logEvent(event: string, params: object): void
    $performance(): void
  }
}

Vue.prototype.$analytics = () => analytics
Vue.prototype.$logEvent = () => logEvent
Vue.prototype.$performance = () => getPerformance(fbapp)
