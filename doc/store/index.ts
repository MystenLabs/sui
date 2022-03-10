import state from './state'
import getters from './getters'
import mutations from './mutations'
import actions from './actions'

export type RootState = ReturnType<typeof state>
export default {
  state,
  mutations,
  getters,
  actions,
}
