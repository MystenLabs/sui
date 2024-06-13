module 0x42::m;

public struct Action<T> { inner: T }

public fun make_action_ref<T>(action: &mut Action<T>): &mut T {
    &mut action.inner.bar
}
