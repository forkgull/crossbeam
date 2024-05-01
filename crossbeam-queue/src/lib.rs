//! Concurrent queues.
//!
//! This crate provides concurrent queues that can be shared among threads:
//!
//! * [`ArrayQueue`], a bounded MPMC queue that allocates a fixed-capacity buffer on construction.
//! * [`SegQueue`], an unbounded MPMC queue that allocates small buffers, segments, on demand.

#![no_std]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms, single_use_lifetimes),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_docs, unsafe_op_in_unsafe_fn)]

#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
extern crate alloc;
#[cfg(crossbeam_loom)]
extern crate loom_crate as loom;
#[cfg(feature = "std")]
extern crate std;

#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
mod array_queue;
#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
mod seg_queue;

#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
pub use crate::{array_queue::ArrayQueue, seg_queue::SegQueue};

#[cfg(crossbeam_loom)]
#[allow(unused_imports, dead_code)]
mod primitive {
    pub(crate) mod cell {
        pub(crate) use loom::cell::UnsafeCell;
    }
    pub(crate) mod sync {
        pub(crate) mod atomic {
            pub(crate) use loom::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering};

            // FIXME: loom does not support compiler_fence at the moment.
            // https://github.com/tokio-rs/loom/issues/117
            // we use fence as a stand-in for compiler_fence for the time being.
            // this may miss some races since fence is stronger than compiler_fence,
            // but it's the best we can do for the time being.
            pub(crate) use self::fence as compiler_fence;
        }
        pub(crate) use loom::sync::Arc;
    }
    pub(crate) use loom::thread_local;
}
#[cfg(target_has_atomic = "ptr")]
#[cfg(not(crossbeam_loom))]
#[allow(unused_imports, dead_code)]
mod primitive {
    pub(crate) mod cell {
        #[derive(Debug)]
        #[repr(transparent)]
        pub(crate) struct UnsafeCell<T>(::core::cell::UnsafeCell<T>);

        // loom's UnsafeCell has a slightly different API than the standard library UnsafeCell.
        // Since we want the rest of the code to be agnostic to whether it's running under loom or
        // not, we write this small wrapper that provides the loom-supported API for the standard
        // library UnsafeCell. This is also what the loom documentation recommends:
        // https://github.com/tokio-rs/loom#handling-loom-api-differences
        impl<T> UnsafeCell<T> {
            #[inline]
            pub(crate) const fn new(data: T) -> Self {
                Self(::core::cell::UnsafeCell::new(data))
            }

            #[inline]
            pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
                f(self.0.get())
            }

            #[inline]
            pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
                f(self.0.get())
            }
        }
    }
    pub(crate) mod sync {
        pub(crate) mod atomic {
            pub(crate) use core::sync::atomic::{compiler_fence, fence, Ordering};

            pub(crate) struct AtomicPtr<T>(::core::sync::atomic::AtomicPtr<T>);

            impl<T> AtomicPtr<T> {
                pub(crate) const fn new(x: *mut T) -> Self {
                    Self(::core::sync::atomic::AtomicPtr::new(x))
                }

                pub(crate) fn load(&self, order: Ordering) -> *mut T {
                    self.0.load(order)
                }

                pub(crate) fn store(&self, value: *mut T, order: Ordering) {
                    self.0.store(value, order);
                }

                pub(crate) fn compare_exchange(
                    &self,
                    current: *mut T,
                    new: *mut T,
                    success: Ordering,
                    failure: Ordering,
                ) -> Result<*mut T, *mut T> {
                    self.0.compare_exchange(current, new, success, failure)
                }

                pub(crate) fn compare_exchange_weak(
                    &self,
                    current: *mut T,
                    new: *mut T,
                    success: Ordering,
                    failure: Ordering,
                ) -> Result<*mut T, *mut T> {
                    self.0.compare_exchange_weak(current, new, success, failure)
                }

                pub(crate) fn with_mut<R>(&mut self, f: impl FnOnce(&mut *mut T) -> R) -> R {
                    f(self.0.get_mut())
                }
            }

            pub(crate) struct AtomicUsize(::core::sync::atomic::AtomicUsize);

            // Similar to UnsafeCell, AtomicUsize has a slightly different API.
            impl AtomicUsize {
                pub(crate) const fn new(x: usize) -> Self {
                    Self(::core::sync::atomic::AtomicUsize::new(x))
                }

                pub(crate) fn load(&self, order: Ordering) -> usize {
                    self.0.load(order)
                }

                pub(crate) fn store(&self, value: usize, order: Ordering) {
                    self.0.store(value, order);
                }

                pub(crate) fn compare_exchange_weak(
                    &self,
                    current: usize,
                    new: usize,
                    success: Ordering,
                    failure: Ordering,
                ) -> Result<usize, usize> {
                    self.0.compare_exchange_weak(current, new, success, failure)
                }

                pub(crate) fn fetch_or(&self, value: usize, order: Ordering) -> usize {
                    self.0.fetch_or(value, order)
                }

                pub(crate) fn with_mut<T>(&mut self, f: impl FnOnce(&mut usize) -> T) -> T {
                    f(self.0.get_mut())
                }
            }
        }
    }
}
