//! Uniprocessor interior mutability primitives
use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};
use riscv::register::sstatus;

/// Wrap a static data structure inside it so that we are
/// able to access it without any `unsafe`.
///
/// We should only use it in uniprocessor.
///
/// In order to get mutable reference of inner data, call
/// `exclusive_access`.
pub struct UPSafeCell<T> {
    /// inner data
    inner: RefCell<T>,
}

unsafe impl<T> Sync for UPSafeCell<T> {}

impl<T> UPSafeCell<T> {
    /// User is responsible to guarantee that inner struct is only used in
    /// uniprocessor.
    pub unsafe fn new(value: T) -> Self {
        Self {
            inner: RefCell::new(value),
        }
    }
    /// Borrow the inner value, with supervisor interrupts disabled meanwhile.
    ///
    /// Panic if the data has been borrowed.
    pub fn exclusive_access(&self) -> UPSafeCellGuard<'_, T> {
        let sie = sstatus::read().sie();
        unsafe {
            sstatus::clear_sie();
        }
        UPSafeCellGuard {
            inner: Some(self.inner.borrow_mut()),
            sie,
        }
    }
}

/// Guard returned by [`UPSafeCell::exclusive_access`].
///
/// Supervisor interrupts stay disabled until the guard is dropped.
pub struct UPSafeCellGuard<'a, T> {
    inner: Option<RefMut<'a, T>>,
    sie: bool,
}

impl<T> Deref for UPSafeCellGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.inner.as_ref().unwrap()
    }
}

impl<T> DerefMut for UPSafeCellGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }
}

impl<T> Drop for UPSafeCellGuard<'_, T> {
    fn drop(&mut self) {
        // Release the borrow before interrupts can be re-enabled.
        self.inner.take();
        if self.sie {
            unsafe {
                sstatus::set_sie();
            }
        }
    }
}
