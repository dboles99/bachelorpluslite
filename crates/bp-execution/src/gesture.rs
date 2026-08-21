//! The one thing that stands between an opened notebook and code running on
//! this machine.

/// Evidence that a person asked for this run, right now.
///
/// A token with no data in it: its whole value is that it can only come into
/// existence at a call site someone wrote deliberately. It is required *by
/// value* by [`crate::run`] and [`crate::run_with_timeout`] -- the only two
/// functions in this crate that start a process -- mirroring exactly how
/// `bp-notebook::UserGesture` is threaded through every `request_run*`
/// method. See this crate's root docs for what that guards against and, just
/// as importantly, what it does not.
///
/// This is a second, independent `UserGesture` type, not a re-export of
/// `bp-notebook`'s. `bp-execution` does not depend on `bp-notebook` --
/// depending on it would be premature for a crate this task builds and tests
/// standalone, per ADR-0040's own framing of this as separable work -- so it
/// defines its own equivalent rather than reach across a boundary that does
/// not exist yet. Whether the two should become one type is a call for
/// whoever wires `bp-execution` into `bp-notebook`/`bp-ui`, with both
/// definitions in front of them.
///
/// Not `Clone`, so one gesture buys one call:
///
/// ```compile_fail
/// fn only_clonable<T: Clone>(_: T) {}
/// only_clonable(bp_execution::UserGesture::from_user_command());
/// ```
///
/// Not `Default` either, so nothing can be conjured from `..Default::default()`:
///
/// ```compile_fail
/// fn only_default<T: Default>() -> T { T::default() }
/// let _: bp_execution::UserGesture = only_default();
/// ```
///
/// And unlike `bp-notebook`'s equivalent, there is no need to separately
/// prove this crate cannot deserialise one: `bp-execution` depends on no
/// serialisation crate at all (see the root docs -- this crate parses
/// nothing), so there is no code path anywhere in it that could read bytes
/// into a `UserGesture` even by mistake.
#[derive(Debug)]
pub struct UserGesture(());

impl UserGesture {
    /// Call this from the handler for a menu item, a key press or a toolbar
    /// button -- somewhere a human's action is on the stack, and this call is
    /// made *because* of that action, not upstream of it. Calling it from a
    /// file-open path, a paste handler or anywhere a value could already be
    /// sitting in memory before a human looked at it is the one mistake this
    /// design exists to make visible in review.
    pub fn from_user_command() -> Self {
        Self(())
    }
}
