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
//! Keleusma 0.2.2's `vm::auto_arena_capacity_for` is not enough on its own. Measured
//! on 2026-10-25, a stream in an arena of exactly that size fails with `OutOfArena` on
//! the resume after its first `Reset`, and a composite argument needs space the figure
//! leaves out. Both were reported to the `v0.2.3` line, which called a host margin a
//! workaround, not a supported bound. The capacity is therefore the sum of four parts,
//! computed in [`ArenaPlan::for_module`].
//!
//! 1. **Footprint.** `auto_arena_capacity_for(module, &[])`, the verifier's figure.
//! 2. **Persistent region.** `required_persistent_capacity_for(module)`, reserved before
//!    the VM is built. Keleusma does not state whether the footprint includes it, so it
//!    is added, at a cost of at most one persistent region of spare capacity.
//! 3. **Regrowth.** Twice the working set, the footprint less the auxiliary and heap
//!    bytes, which is the operand stack and the call frames. In every script measured,
//!    the excess over the footprint was one doubling of the operand stack, which this
//!    covers with frame regrowth besides. **Empirical, not proven.** The derivation is
//!    in `docs/architecture/BINDING_DESIGN.md`.
//! 4. **Argument budget.** A host-chosen number of bytes for composite arguments, such
//!    as tuples carrying text. Their cost depends on the host's data and grows with the
//!    number of yields in one stream iteration, so no figure derived from the module can
//!    cover it. An argument that does not fit fails with Keleusma's `OutOfArena`, a clean
//!    error, never undefined behaviour.
//!
//! The footprint includes the memory bound declared for each native, passed in the
//! order of the module's `native_names`, which is how `auto_arena_capacity_for` takes
//! them. A bound raised after construction is re-checked with `verify_resources` and
//! refused if this arena cannot hold it.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use keleusma::Arena;
use keleusma::bytecode::{Module, Value};
use keleusma::verify::module_runtime_footprint;
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

/// The parts of an arena's capacity, as described in the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArenaPlan {
    /// The verifier's figure from `auto_arena_capacity_for`.
    pub footprint: usize,
    /// The persistent region for the module's private data.
    pub persistent: usize,
    /// Spare capacity for the operand stack and frames to regrow once.
    pub regrowth: usize,
    /// The host's budget for composite arguments.
    pub argument_bytes: usize,
    /// The sum of the four parts above.
    pub capacity: usize,
}

impl ArenaPlan {
    /// Plan the arena for `module` with `argument_bytes` reserved for arguments and
    /// `native_wcmu` holding each native's memory bound in `native_names` order.
    pub fn for_module(
        module: &Module,
        argument_bytes: usize,
        native_wcmu: &[u32],
    ) -> Result<Self, OwnedVmError> {
        let footprint = auto_arena_capacity_for(module, native_wcmu).map_err(OwnedVmError::Vm)?;
        let heap = module_runtime_footprint(module, native_wcmu)
            .map_err(|e| {
                OwnedVmError::Vm(VmError::VerifyError(format!(
                    "{}: {}",
                    e.chunk_name, e.message
                )))
            })?
            .max_heap_bytes;
        // `auto_arena_capacity_for` adds these two to the stack and frame bytes, so
        // subtracting them leaves the working set. Saturating, because a footprint
        // smaller than its own parts would be Keleusma's inconsistency, and a smaller
        // margin is then the conservative reading only if the arena still verifies,
        // which `Vm::new` checks.
        let working = footprint
            .saturating_sub(module.aux_arena_bytes as usize)
            .saturating_sub(heap as usize);
        let persistent = required_persistent_capacity_for(module);
        let regrowth = working
            .checked_mul(2)
            .ok_or(OwnedVmError::CapacityOverflow)?;
        let capacity = [persistent, regrowth, argument_bytes]
            .into_iter()
            .try_fold(footprint, usize::checked_add)
            .ok_or(OwnedVmError::CapacityOverflow)?;
        Ok(Self {
            footprint,
            persistent,
            regrowth,
            argument_bytes,
            capacity,
        })
    }
}

/// A verified Keleusma VM and the arena it borrows, owned as one value.
pub struct OwnedVm {
    cell: ArenaAndVm,
    plan: ArenaPlan,
}

impl OwnedVm {
    /// Size an arena for `module`, verify the module against it, and own both.
    ///
    /// `argument_bytes` is the part of the arena reserved for composite arguments, as
    /// the module documentation describes.
    pub fn new(
        module: Module,
        argument_bytes: usize,
        native_wcmu: &[u32],
    ) -> Result<Self, OwnedVmError> {
        let plan = ArenaPlan::for_module(&module, argument_bytes, native_wcmu)?;
        let capacity = plan.capacity;
        let persistent = plan.persistent;
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
        Ok(Self { cell, plan })
    }

    /// The arena's total capacity in bytes.
    pub fn arena_capacity(&self) -> usize {
        self.cell.borrow_owner().capacity()
    }

    /// The arena, for reading a result that lives in it.
    pub fn arena(&self) -> &Arena {
        self.cell.borrow_owner()
    }

    /// How the capacity was made up.
    pub fn plan(&self) -> ArenaPlan {
        self.plan
    }

    /// Call the module's entry point.
    ///
    /// The returned state may hold an arena-resident composite, which is valid only
    /// until the next call or resume on this VM, so callers must convert it first.
    pub fn call(&mut self, args: &[Value]) -> Result<VmState, VmError> {
        self.cell.with_dependent_mut(|_arena, vm| vm.call(args))
    }

    /// Keleusma's worst-case execution time for one stream iteration, in its nominal
    /// cost units, including the declared bounds of any registered natives.
    pub fn wcet_per_iteration(&self) -> Result<u32, VmError> {
        self.cell
            .with_dependent(|_arena, vm| vm.wcet_per_iteration())
    }

    /// Register `func` as the native `name`, replacing any earlier registration.
    pub fn register_native<Func>(&mut self, name: &str, func: Func)
    where
        Func:
            for<'b> Fn(&keleusma::vm::NativeCtx<'b>, &[Value]) -> Result<Value, VmError> + 'static,
    {
        self.cell
            .with_dependent_mut(|_arena, vm| vm.register_native_with_ctx_closure(name, func));
    }

    /// Declare the worst-case time and arena memory of one call of the native `name`.
    pub fn set_native_bounds(
        &mut self,
        name: &str,
        wcet: u32,
        wcmu_bytes: u32,
    ) -> Result<(), VmError> {
        self.cell
            .with_dependent_mut(|_arena, vm| vm.set_native_bounds(name, wcet, wcmu_bytes))
    }

    /// Check the module, with the natives' current bounds, against this arena.
    pub fn verify_resources(&self) -> Result<(), VmError> {
        self.cell.with_dependent(|_arena, vm| vm.verify_resources())
    }

    /// Resume after a yield or a reset, with the same validity rule as [`Self::call`].
    pub fn resume(&mut self, input: Value) -> Result<VmState, VmError> {
        self.cell.with_dependent_mut(|_arena, vm| vm.resume(input))
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
    use keleusma::compiler::compile;
    use keleusma::lexer::tokenize;
    use keleusma::parser::parse;

    use super::*;

    /// The iteration count roadmap task 3 requires.
    const LONG_RUN: i64 = 10_000;
    /// Enough iterations to pass through several resets, short enough for Miri.
    const SHORT_RUN: i64 = 20;
    /// The argument budget the Python class uses by default.
    const BUDGET: usize = crate::DEFAULT_ARGUMENT_BYTES;

    fn module(source: &str) -> Module {
        let tokens = tokenize(source).expect("lex");
        let program = parse(&tokens).expect("parse");
        compile(&program).expect("compile")
    }

    fn finished_int(vm: &mut OwnedVm) -> i64 {
        match vm.call(&[]).expect("call") {
            VmState::Finished(Value::Int(n)) => n,
            other => panic!("expected a finished Int, got {other:?}"),
        }
    }

    fn text(value: &str) -> Value {
        Value::StaticStr(value.to_string())
    }

    /// Drive a stream for `iterations` steps, failing on the first error.
    fn drive(source: &str, budget: usize, iterations: i64, argument: &dyn Fn(i64) -> Value) {
        let mut vm = OwnedVm::new(module(source), budget, &[]).expect("new");
        vm.call(&[argument(0)]).expect("call");
        for step in 1..iterations {
            match vm.resume(argument(step)) {
                Ok(VmState::Yielded(_) | VmState::Reset) => {}
                other => panic!("step {step} of `{source}` failed with {other:?}"),
            }
        }
    }

    /// Streams taking a word, from the measurements in BINDING_DESIGN.md.
    const WORD_STREAMS: [&str; 7] = [
        "loop main(e: Word) -> Word { let n = yield e + 1; n }",
        "loop main(e: Word) -> Word { let a = yield e; let b = yield a + 1; b }",
        "fn f(x: Word) -> Word { g(x) + 1 }\nfn g(x: Word) -> Word { x * 2 }\n\
         loop main(e: Word) -> Word { let n = yield f(e); n }",
        "private data d { n: Word }\nloop main(e: Word) -> Word { d.n = e; let x = yield d.n; x }",
        "loop main(e: Word) -> Word { let n = yield ((e + 1) * (e + 2)) + ((e + 3) * ((e + 4) - (e + 5))); n }",
        "loop main(e: Word) -> Word { let a = yield e; let b = yield a; let c = yield b; let d = yield c; d }",
        "fn a(x: Word) -> Word { b(x) + c(x) }\nfn b(x: Word) -> Word { c(x) * 2 }\n\
         fn c(x: Word) -> Word { d(x) - 1 }\nfn d(x: Word) -> Word { x + 7 }\n\
         loop main(e: Word) -> Word { let n = yield a(e); n }",
    ];

    /// A stream taking a tagged tuple of text, in Dromon's event shape.
    const PAIR_STREAM: &str =
        "loop main(e: (Text, Text)) -> (Text, Text) { let n = yield (\"send\", e.1); n }";

    fn pair(step: i64) -> Value {
        let body = if step % 2 == 0 {
            "hello"
        } else {
            "a longer piece of host text"
        };
        Value::tuple(vec![text("tokens"), text(body)])
    }

    fn check_word_streams(iterations: i64) {
        for source in WORD_STREAMS {
            drive(source, 0, iterations, &Value::Int);
        }
    }

    #[test]
    fn word_streams_survive_resets_without_an_argument_budget() {
        check_word_streams(SHORT_RUN);
    }

    #[test]
    #[cfg_attr(miri, ignore = "10,000 iterations are too slow under Miri")]
    fn word_streams_run_ten_thousand_iterations() {
        check_word_streams(LONG_RUN);
    }

    #[test]
    fn a_tuple_stream_survives_resets_within_the_default_budget() {
        drive(PAIR_STREAM, BUDGET, SHORT_RUN, &pair);
    }

    #[test]
    #[cfg_attr(miri, ignore = "10,000 iterations are too slow under Miri")]
    fn a_tuple_stream_runs_ten_thousand_iterations_within_the_default_budget() {
        drive(PAIR_STREAM, BUDGET, LONG_RUN, &pair);
    }

    #[test]
    fn the_verifier_footprint_alone_fails_after_the_first_reset() {
        // The reason for the regrowth margin. If Keleusma fixes its footprint, this
        // test fails, and the margin can be reconsidered rather than kept by habit.
        let source = WORD_STREAMS[0];
        let built = module(source);
        let footprint = auto_arena_capacity_for(&built, &[]).expect("footprint");
        let arena = Arena::try_with_capacity(footprint).expect("arena");
        let mut vm = Vm::new(built, &arena).expect("verified");
        assert!(matches!(vm.call(&[Value::Int(1)]), Ok(VmState::Yielded(_))));
        assert!(matches!(vm.resume(Value::Int(2)), Ok(VmState::Reset)));
        assert!(matches!(
            vm.resume(Value::Int(3)),
            Err(VmError::OutOfArena(_))
        ));
    }

    #[test]
    fn an_argument_larger_than_the_budget_fails_cleanly() {
        let mut vm = OwnedVm::new(module(PAIR_STREAM), 64, &[]).expect("new");
        let large = Value::tuple(vec![text("tokens"), text(&"x".repeat(4096))]);
        assert!(matches!(vm.call(&[large]), Err(VmError::OutOfArena(_))));
    }

    #[test]
    fn the_plan_adds_its_parts() {
        let built = module(WORD_STREAMS[3]);
        let plan = ArenaPlan::for_module(&built, 100, &[]).expect("plan");
        assert_eq!(
            plan.footprint,
            auto_arena_capacity_for(&built, &[]).expect("fp")
        );
        assert_eq!(plan.persistent, required_persistent_capacity_for(&built));
        assert!(plan.persistent > 0);
        assert!(plan.regrowth > 0);
        assert_eq!(plan.argument_bytes, 100);
        assert_eq!(
            plan.capacity,
            plan.footprint + plan.persistent + plan.regrowth + plan.argument_bytes
        );
        let vm = OwnedVm::new(built, 100, &[]).expect("new");
        assert_eq!(vm.arena_capacity(), plan.capacity);
        assert_eq!(vm.plan(), plan);
    }

    #[test]
    fn an_argument_budget_that_overflows_is_refused() {
        let built = module(WORD_STREAMS[0]);
        assert!(matches!(
            OwnedVm::new(built, usize::MAX, &[]),
            Err(OwnedVmError::CapacityOverflow)
        ));
    }

    #[test]
    fn a_vm_survives_being_moved_away_from_where_it_was_built() {
        // The VM borrows the arena by address. Moving the owner into a Box and back
        // out must not invalidate that borrow, which self_cell's heap-allocated
        // owner guarantees.
        let vm = OwnedVm::new(module("fn main() -> Word { 40 + 2 }"), 0, &[]).expect("new");
        let mut moved = *Box::new(vm);
        assert_eq!(finished_int(&mut moved), 42);
    }

    #[test]
    fn private_data_is_dropped_while_the_arena_is_alive() {
        // The VM's Drop walks the arena's persistent region and drops each private
        // slot in place. With the wrong order that walk would read freed memory.
        // This test reaches the walk, but an ordinary test run cannot observe a
        // use after free, so it shows only that the path runs. Miri, which runs this
        // suite in CI, detects the defect itself.
        let source = "struct Point { x: Word, y: Word }\n\
                      private data d { p: Point }\n\
                      fn main() -> Word { d.p = Point { x: 3, y: 4 }; d.p.x + d.p.y }";
        let mut vm = OwnedVm::new(module(source), 0, &[]).expect("new");
        assert_eq!(finished_int(&mut vm), 7);
        drop(vm);
    }
}
