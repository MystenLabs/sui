const navConfig = require('../nav.config.js')
// filter the menu items based content folder structure
// TODO rewrite this
interface MenuFilter {
  slug: string
  title: string
  path: string
  categoryName?: string
  topMenu?: boolean
  categoryOrder?: number
  itemOder?: number
  subCategoryName?: string
}

const primaryMenufilterName = 'topMenu'

export const externalSideMenu = () => {
  const sideMenuData = navConfig.docs
  const catOrder = Object.keys(sideMenuData)
  const reps = catOrder.map((keyName: string, index: number) => {
    const external = sideMenuData[keyName].filter((itm: any) => itm.link)
    if (!external) return []
    return external.map((itm: any) => {
      return {
        slug: itm.label,
        path: itm.link,
        title: itm.label,
        external: true,
        categoryOrder: index + 1,
        categoryName: keyName,
      }
    })
  })
  return reps.length > 0 ? reps.filter((itm: any) => itm.length > 0)[0] : []
}

const generateAllMenuItemsFromList = () => {
  /// get Category order from config file
  const menuData = navConfig.docs
  const catOrder = Object.keys(menuData)

  const menuOrderData = catOrder.map((itm, i) => {
    return menuData[itm].map((element: any, index: number) => {
      if (element.items && element.items.length) {
        return element.items.map((subElement: any, subIndex: number) => {
          return {
            categoryName: itm,
            categoryIndex: i + 1,
            menuItemIndex: index + 1,
            title: subElement.label,
            external: subElement.link ? true : false,
            subCategoryName: element.title,
            path: subElement.link || '/' + subElement.fileName,
          }
        })
      }

      return {
        categoryName: itm,
        categoryIndex: i + 1,
        menuItemIndex: index + 1,
        external: element.link ? true : false,
        title: element.label,
        path:  element.link || '/' + element.fileName ,
      }
    })
  })

  return menuOrderData.flat(3)
}

// TODO remove any broken links from the menu
export const menuFilter = (menuItems1: Array<MenuFilter>): any => {

  if (!menuItems1 || menuItems1.length === 0) return {}
  /// Primary menu items
  const primaryMenu = menuItems1.filter((item) => item[primaryMenufilterName])
  /// Secondary menu items and sort by categoryOrder
  // return only non-topMenu items and items with categoryName
  const menuItems = generateAllMenuItemsFromList()

  // Remove any menu item from nav.congig without a corresponding page in the docs folder or that isnt a external link
  // Avoiding broken links
  const listOfInternalLinks = menuItems1.filter((item:any) => !item.external).map((item:any) => item.path)
  const secondaryMenu = menuItems
    .filter(
      (item: any) => item.external || listOfInternalLinks.includes(item.path)
    )
    .sort((a: any, b: any) => (a.categoryOrder > b.categoryOrder ? 1 : -1))

  /// filter by categoryOrder and remove duplicates
  const getAllCateryName = Object.keys(navConfig.docs)

  return {
    primaryMenu,

    /// dont return empty category ie items in nav.config.js but not in content folder&& !item.subCategoryName
    devDocMenu: getAllCateryName
      .map((categoryName) => {
        const subMenu = secondaryMenu
          .filter(
            (item) =>
              item.categoryName === categoryName && !item.subCategoryName
          )
          .sort((a: any, b: any) => (a.itemOder > b.itemOder ? 1 : -1))
        const subSubMenu = secondaryMenu
          .filter(
            (item) => item.categoryName === categoryName && item.subCategoryName
          )
          .sort((a: any, b: any) => (a.itemOder > b.itemOder ? 1 : -1))

        const subCategoryName = Array.from(
          new Set(subSubMenu.map((item) => item.subCategoryName))
        )
        const dataSub = subCategoryName.map((subCategoryName) => ({
          title: subCategoryName,
          path: false,
          submenu: subSubMenu.filter(
            (itm) => itm.subCategoryName === subCategoryName
          ),
        }))

        return categoryName
          ? {
              menu: [...subMenu, ...dataSub],
              subMenuTitle: categoryName,
            }
          : false
      })
      .filter((item) => item),
  }
}

  // TODO: Redo this
  /// Auto Generate Front Matter infomation for each page in docs folder
  /// search for a page buy title and return the front matter information
export const menuOrderGenerator = (menuName: string, path: string): any => {
  const pathName = path ? path.substring(1) : menuName.toLowerCase()

  /// get Category order from config file
  const catOrder = Object.keys(navConfig.docs)
  const menuData = navConfig.docs

  /// find menu item by name and get the index of the category and the index of the menu item in the config file
  const menuOrderData = catOrder
    .map((itm, i) => {
      if (!menuData[itm] || !menuData[itm].length) return false

      /// use list name for submenu title and title for child submenu
      const menuItems = menuData[itm]
        .map((name: any) => {
          return name.items
            ? name.items.map((im: any) => im.fileName.toLowerCase())
            : name.fileName
            ? name.fileName.toLowerCase()
            : false
        })
        .flat(3)
        .filter((im: any) => im.length)

      if (!menuItems.includes(pathName)) return false

      const menuItemIndex = menuItems
        .map((item: any, index: number) => {
          if (item !== pathName) return false

          /// Get nav items with type object
          const subMenuArr = menuData[itm].filter((im: any) => im.items)

          /// add subCategoryName title name base
          const sunMenuCat = subMenuArr
            .map((im: any) => {
              return im.items ? { subCategoryName: im.title } : false
            })
            .filter((im: any) => im.length)

          return {
            categoryIndex: i + 1,
            menuItemIndex: index + 1,
            categoryName: itm,
            /// include subCategoryName if sidemenu have items
            ...(sunMenuCat.length && sunMenuCat[0]),
          }
        })
        .filter((item: any) => item)

      return menuItemIndex[0]
    })
    .filter((item: any) => item)

  return menuOrderData[0]
}
