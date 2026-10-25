//! A Keleusma VM that owns the arena it borrows.
//!
//! `keleusma::vm::Vm::new` takes `&'arena Arena`, and a `#[pyclass]` cannot carry a
//! borrowed lifetime, so the binding needs one value that owns both the arena and the
//! VM that borrows it. [`OwnedVm`] is that value, built on `self_cell`. The alternative
//! of leaking the arena with `Box::leak` was rejected in
//! `docs/architecture/BINDING_DESIGN.md`, because a long-running Python program that
//! creates VMs repeatedly would leak one arena per VM.
//!
//! # Invariants
//!
//! - **Drop order.** `self_cell` drops the dependent before the owner. The VM's own
//!   `Drop` reads the arena's persistent region to drop private data slots, so the VM
//!   must be dropped while the arena is alive. `self_cell` provides exactly that order,
//!   and no code in this module drops either half separately.
//! - **Address stability.** `self_cell` stores the owner in a heap allocation that does
//!   not move for the life of the cell, so the `&Arena` held by the VM stays valid when
//!   the [`OwnedVm`] value itself is moved, for example into a Python object.
//! - **Verified admission only.** Construction goes through `Vm::new`, which runs the
//!   structural and resource-bound verifier. `Vm::new_unchecked` is never used.
//! - **No cross-thread use.** `Arena` holds `Cell` fields and a raw pointer, so neither
//!   it nor the VM is `Send` or `Sync`, and the compiler therefore rejects any attempt to
//!   move an [`OwnedVm`] across threads. The Python class that holds one is declared
//!   `unsendable` to match.
//!
//! # Arena sizing
//!
//! The arena is sized as the module's runtime footprint from
//! `vm::auto_arena_capacity_for` plus the persistent region from
//! `vm::required_persistent_capacity_for`, and the persistent region is reserved before
//! the VM is built. Whether the footprint already includes the persistent region is not
//! stated by Keleusma 0.2.2, so the two are added. The cost of that assumption is at
//! most one persistent region of extra capacity, never an undersized arena.
//!
//! The footprint is computed with no native memory bounds, because no natives exist at
//! construction. A native registered later with a declared memory bound is not covered
//! by this capacity. That is a known gap for the native registration task, recorded in
//! the design document.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use keleusma::Arena;
use keleusma::bytecode::Module;
use keleusma::vm::{
    Vm, VmError, VmState, auto_arena_capacity_for, required_persistent_capacity_for,
};
use self_cell::self_cell;

/// The VM type the arena owner lends to. The bytecode lifetime is `'static` because
/// the VM is built from an owned `Module`, so only the arena lifetime is borrowed.
type BorrowingVm<'arena> = Vm<'static, 'arena>;

self_cell!(
    /// The arena together with the VM that borrows it. Not covariant, because nothing
    /// in this binding needs to shorten the arena lifetime, and declaring it covariant
    /// would be an unverified claim about Keleusma's internal types.
    struct ArenaAndVm {
        owner: Arena,

        #[not_covariant]
        dependent: BorrowingVm,
    }
);

/// The number of [`OwnedVm`] values alive in this process.
///
/// It exists so a test can confirm that dropping the Python object really frees the
/// arena and the VM, which a leaking design would fail. `Relaxed` ordering suffices
/// because the count is a diagnostic and orders no other memory.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// The number of [`OwnedVm`] values currently alive.
pub fn live_count() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// Why an [`OwnedVm`] could not be constructed.
#[derive(Debug)]
pub enum OwnedVmError {
    /// The verifier rejected the module, or the arena could not be sized for it.
    Vm(VmError),
    /// The arena's backing buffer could not be allocated. Reported rather than
    /// aborting, which `Arena::with_capacity` would do.
    ArenaAllocation { capacity: usize },
    /// The persistent region did not fit inside the arena. This indicates a sizing
    /// defect in this module, since the capacity includes the persistent region.
    PersistentRegion { capacity: usize, persistent: usize },
    /// The capacity computation overflowed `usize`.
    CapacityOverflow,
}

impl fmt::Display for OwnedVmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vm(e) => write!(f, "{e:?}"),
            Self::ArenaAllocation { capacity } => {
                write!(f, "could not allocate an arena of {capacity} bytes")
            }
            Self::PersistentRegion {
                capacity,
                persistent,
            } => write!(
                f,
                "a persistent region of {persistent} bytes does not fit an arena of \
                 {capacity} bytes"
            ),
            Self::CapacityOverflow => f.write_str("the arena capacity overflowed usize"),
        }
    }
}

impl std::error::Error for OwnedVmError {}

/// A verified Keleusma VM and the arena it borrows, owned as one value.
pub struct OwnedVm {
    cell: ArenaAndVm,
}

impl OwnedVm {
    /// Size an arena for `module`, verify the module against it, and own both.
    pub fn new(module: Module) -> Result<Self, OwnedVmError> {
        let footprint = auto_arena_capacity_for(&module, &[]).map_err(OwnedVmError::Vm)?;
        let persistent = required_persistent_capacity_for(&module);
        let capacity = footprint
            .checked_add(persistent)
            .ok_or(OwnedVmError::CapacityOverflow)?;
        let mut arena = Arena::try_with_capacity(capacity)
            .map_err(|_| OwnedVmError::ArenaAllocation { capacity })?;
        arena
            .resize_persistent(persistent)
            .map_err(|_| OwnedVmError::PersistentRegion {
                capacity,
                persistent,
            })?;
        let cell =
            ArenaAndVm::try_new(arena, |arena| Vm::new(module, arena)).map_err(OwnedVmError::Vm)?;
        LIVE.fetch_add(1, Ordering::Relaxed);
        Ok(Self { cell })
    }

    /// The arena's total capacity in bytes.
    pub fn arena_capacity(&self) -> usize {
        self.cell.borrow_owner().capacity()
    }

    /// Call the module's entry point with no arguments.
    ///
    /// Arguments and value conversion arrive with later tasks. The returned state may
    /// hold an arena-resident composite, which is valid only until the next call on
    /// this VM, so callers must convert it before calling again.
    pub fn call_no_args(&mut self) -> Result<VmState, VmError> {
        self.cell.with_dependent_mut(|_arena, vm| vm.call(&[]))
    }
}

impl Drop for OwnedVm {
    fn drop(&mut self) {
        // The cell itself drops after this body, VM first and arena second.
        LIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use keleusma::bytecode::Value;
    use keleusma::compiler::compile;
    use keleusma::lexer::tokenize;
    use keleusma::parser::parse;

    use super::*;

    fn module(source: &str) -> Module {
        let tokens = tokenize(source).expect("lex");
        let program = parse(&tokens).expect("parse");
        compile(&program).expect("compile")
    }

    fn finished_int(vm: &mut OwnedVm) -> i64 {
        match vm.call_no_args().expect("call") {
            VmState::Finished(Value::Int(n)) => n,
            other => panic!("expected a finished Int, got {other:?}"),
        }
    }

    #[test]
    fn a_vm_survives_being_moved_away_from_where_it_was_built() {
        // The VM borrows the arena by address. Moving the owner into a Box and back
        // out must not invalidate that borrow, which self_cell's heap-allocated
        // owner guarantees.
        let vm = OwnedVm::new(module("fn main() -> Word { 40 + 2 }")).expect("new");
        let mut moved = *Box::new(vm);
        assert_eq!(finished_int(&mut moved), 42);
    }

    #[test]
    fn private_data_is_dropped_while_the_arena_is_alive() {
        // The VM's Drop walks the arena's persistent region and drops each private
        // slot in place. With the wrong order that walk would read freed memory.
        // This test reaches the walk, but an ordinary test run cannot observe a
        // use after free, so it shows only that the path runs. Miri or
        // AddressSanitizer would be needed to detect the defect itself.
        let source = "struct Point { x: Word, y: Word }\n\
                      private data d { p: Point }\n\
                      fn main() -> Word { d.p = Point { x: 3, y: 4 }; d.p.x + d.p.y }";
        let mut vm = OwnedVm::new(module(source)).expect("new");
        assert_eq!(finished_int(&mut vm), 7);
        drop(vm);
    }

    #[test]
    fn the_arena_reserves_the_persistent_region() {
        let source = "private data d { a: Word, b: Word }\n\
                      fn main() -> Word { d.a = 1; d.b = 2; d.a + d.b }";
        let built = module(source);
        let persistent = required_persistent_capacity_for(&built);
        assert!(persistent > 0);
        let mut vm = OwnedVm::new(built).expect("new");
        assert!(vm.arena_capacity() >= persistent);
        assert_eq!(finished_int(&mut vm), 3);
    }
}
