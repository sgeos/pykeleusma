//! Natives declared `use external`, served by Python callables.
//!
//! Keleusma classifies a native imported with `use external module::name` as
//! external: the host attests how many times it may be called per stream iteration,
//! rather than its cost per call. Keleusma 0.2.2 serves such a native only through
//! `Vm::register_external_native`, which takes a plain function pointer with no
//! context, so it can neither capture a Python callable nor reach the arena that a
//! text or composite argument lives in.
//!
//! KELEUSMA-0.2.2: the 0.2.3 branch keeps the function pointer. A closure-based
//! registration with a native context, requested of Keleusma, would retire this module.
//!
//! # How the binding bridges it
//!
//! A fixed pool of [`MAX_EXTERNAL`] trampoline functions stands in for the function
//! pointers. While a VM runs, it pushes a [`Frame`] onto a thread-local stack holding
//! its table of closures, one per external native in trampoline order, and a pointer to
//! its arena. Trampoline `k` calls closure `k` of the top frame with a native context
//! built from that arena, which is the context every other native of the binding gets.
//! The frame is popped when the run returns, even by unwinding.
//!
//! # Soundness
//!
//! - **The arena pointer.** It is taken from the shared reference to the arena that
//!   the run itself holds, and is dereferenced only by a trampoline, which Keleusma
//!   calls only during that run, while the arena is alive and only shared-borrowed.
//! - **Nesting.** A native that runs another VM pushes that VM's frame and pops it
//!   before returning, so the top frame is always the VM whose script made the call.
//!   The same VM cannot be re-entered, because PyO3 refuses the second borrow.
//! - **Borrows.** The stack is borrowed only to clone what a call needs, and the
//!   borrow is released before Python runs, so a nested VM can push its own frame.
//! - **Threads.** The stack is thread-local, and a VM never leaves its thread.

use std::cell::RefCell;
use std::rc::Rc;

use keleusma::Arena;
use keleusma::bytecode::Value;
use keleusma::vm::{NativeCtx, VmError};

use crate::compat::Widths;

/// How many external natives one module may declare, the size of the trampoline pool.
pub const MAX_EXTERNAL: usize = 16;

/// A native's closure, as the binding builds one for every Python native.
pub type Closure = Rc<dyn for<'b> Fn(&NativeCtx<'b>, &[Value]) -> Result<Value, VmError>>;

/// One VM's external natives, in trampoline order, and the widths they read at.
#[derive(Default)]
pub struct Table {
    pub closures: Vec<Closure>,
    pub widths: Option<Widths>,
}

/// A VM's table, shared between the VM and the frames its runs push.
pub type SharedTable = Rc<RefCell<Table>>;

/// What a trampoline needs from the VM whose run is in progress.
struct Frame {
    table: SharedTable,
    arena: *const Arena,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Pops the frame it was created with when dropped, including during unwinding.
pub struct Entered(());

impl Drop for Entered {
    fn drop(&mut self) {
        FRAMES.with(|frames| {
            frames.borrow_mut().pop();
        });
    }
}

/// Make `table` and `arena` the context for external natives until the returned
/// guard is dropped. `arena` must outlive the guard, which the caller ensures by
/// dropping the guard before the run that borrows the arena ends.
pub fn enter(table: &SharedTable, arena: &Arena) -> Entered {
    FRAMES.with(|frames| {
        frames.borrow_mut().push(Frame {
            table: Rc::clone(table),
            arena: std::ptr::from_ref(arena),
        });
    });
    Entered(())
}

/// Call external native `index` of the VM whose run is in progress.
fn dispatch(index: usize, args: &[Value]) -> Result<Value, VmError> {
    let found = FRAMES.with(|frames| {
        let frames = frames.borrow();
        let frame = frames.last()?;
        let table = frame.table.borrow();
        let closure = table.closures.get(index).cloned()?;
        Some((closure, table.widths?, frame.arena))
    });
    let Some((closure, widths, arena)) = found else {
        return Err(VmError::InvalidBytecode(format!(
            "external native {index} was called outside a run of its VM"
        )));
    };
    // SAFETY: the pointer came from the shared reference the current run holds, as
    // `enter` records, and the run is still in progress, because Keleusma calls a
    // trampoline only from inside it. Only shared references to the arena exist.
    let arena = unsafe { &*arena };
    let ctx = crate::compat::native_ctx(arena, widths);
    closure(&ctx, args)
}

macro_rules! trampolines {
    ($($index:literal),* $(,)?) => {
        /// The function pointers registered with Keleusma, one per table index.
        pub const TRAMPOLINES: [fn(&[Value]) -> Result<Value, VmError>; MAX_EXTERNAL] = [
            $(|args: &[Value]| dispatch($index, args)),*
        ];
    };
}

trampolines!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trampoline_outside_a_run_is_an_error() {
        assert!(matches!(
            TRAMPOLINES[0](&[]),
            Err(VmError::InvalidBytecode(_))
        ));
    }

    #[test]
    fn frames_nest_and_each_guard_pops_its_own() {
        let arena = Arena::with_capacity(4096);
        let outer: SharedTable = Rc::default();
        let inner: SharedTable = Rc::default();
        let depth = || FRAMES.with(|f| f.borrow().len());
        let first = enter(&outer, &arena);
        {
            let _second = enter(&inner, &arena);
            assert_eq!(depth(), 2);
        }
        assert_eq!(depth(), 1);
        drop(first);
        assert_eq!(depth(), 0);
    }

    #[test]
    fn a_trampoline_calls_the_top_frames_closure() {
        let arena = Arena::with_capacity(4096);
        let widths = Widths {
            word_bytes: 8,
            float_bytes: 8,
            addr_bytes: 8,
        };
        let table: SharedTable = Rc::new(RefCell::new(Table {
            closures: vec![Rc::new(|_: &NativeCtx<'_>, args: &[Value]| {
                Ok(Value::Int(args.len() as i64))
            })],
            widths: Some(widths),
        }));
        let _entered = enter(&table, &arena);
        assert!(matches!(
            TRAMPOLINES[0](&[Value::Unit, Value::Unit]),
            Ok(Value::Int(2))
        ));
        assert!(TRAMPOLINES[1](&[]).is_err());
    }
}
