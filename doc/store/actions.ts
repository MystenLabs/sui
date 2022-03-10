import { ActionTree } from 'vuex'
import { menuFilter, externalSideMenu } from '../utils/menuFilter'
import { siteConfig } from '../site.config'
import { RootState } from './index'
import navConfig from '~/nav.config'
import { apiReferenceService } from '~/utils/apiReferenceFilter'

const actions: ActionTree<RootState, RootState> = {
  // Get all Markdown files in Content -> Docs folder and store them in state
  async docSideMenuInit({ commit }: any, context) {
    const subMenu = await context
      .$content('docs', { deep: true })
      .only(['title', 'path', 'dir'])
      .fetch()
    return commit('setDevDocMenuState', subMenu)
  },

  updateSearChState({ commit }: any, id: any) {
    return commit('setSearchState', id)
  },

  /// Get site wide settings and store them in the state
  async nuxtServerInit({ commit }: any, context) {
    const coreData = await context
      .$content('', { deep: true })
      // .where({ topMenu: true })
      .only([
        'title',
        'path',
        'slug',
        'topMenu',
        'dir',
        'categoryOrder',
        'itemOder',
        'categoryName',
        'subCategoryName',
      ])
      .fetch()

    const externalSideMenuData = externalSideMenu()
    const groupedMenu = menuFilter([...coreData, ...externalSideMenuData])

    commit ('setApiReference', apiReferenceService())
    commit('setCore', groupedMenu.primaryMenu)
    commit('setSideMenu', navConfig.sideMenu)
    commit('setFooter', siteConfig.footerData)
    commit('setMenuState', siteConfig.headerData.menu)
    commit('setHomeData', siteConfig.HomePage)
    return commit('setDevDocMenuState', groupedMenu.devDocMenu)
  },
}

export default actions
