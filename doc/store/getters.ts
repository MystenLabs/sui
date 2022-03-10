import { GetterTree } from 'vuex'
import { RootState } from './index'

const getters: GetterTree<RootState, RootState> = {
  getCore: (state: RootState) => state.core,
  getFooter: (state: RootState) => state.footer,
  navMenu: (state: RootState) => state.menu,
  devDocMenu: (state: RootState) => state.docMenu,
  getSearchState: (state: RootState) => state.search,
  getHomeData: (state: RootState) => state.homeData,
}

export default getters
