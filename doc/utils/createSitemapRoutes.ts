export const createSitemapRoutes = async () => {
  const routes = []
  let posts: any = []
  const { $content } = require('@nuxt/content')
  if (posts === null || posts.length === 0)
    posts = await $content({ deep: true }).fetch()
  for (const post of posts) {
    routes.push(`${post.path}/`)
  }
  return routes
}
