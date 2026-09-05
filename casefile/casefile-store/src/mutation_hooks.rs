use std::{cell::RefCell, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Boundary {
    Attempt,
    Locked,
    Read,
    Commit,
    Result,
    Write,
}
type Hook = Box<dyn FnMut(Boundary, &Path, &str)>;
thread_local! { static HOOK: RefCell<Option<Hook>> = RefCell::new(None); }

pub(super) fn set(hook: impl FnMut(Boundary, &Path, &str) + 'static) {
    HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}
pub(super) fn clear() {
    HOOK.with(|slot| *slot.borrow_mut() = None);
}
pub(super) fn event(boundary: Boundary, root: &Path, path: &str) {
    let hook = HOOK.with(|slot| slot.borrow_mut().take());
    if let Some(mut hook) = hook {
        hook(boundary, root, path);
        HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
    }
}

thread_local! { static WRITE_FAILURE: RefCell<Option<String>> = const { RefCell::new(None) }; }
pub(super) fn fail_write(path: String) {
    WRITE_FAILURE.with(|slot| *slot.borrow_mut() = Some(path));
}
pub(super) fn writing(root: &Path, path: &str) -> Result<(), crate::StoreError> {
    let fail = WRITE_FAILURE.with(|slot| {
        if slot.borrow().as_deref() == Some(path) {
            slot.borrow_mut().take();
            true
        } else {
            false
        }
    });
    if fail {
        return Err(std::io::Error::other("injected atomic write failure").into());
    }
    event(Boundary::Write, root, path);
    Ok(())
}
