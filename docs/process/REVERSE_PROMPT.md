# Reverse Prompt

> **Navigation**: [Process](./README.md)

A state file, overwritten whole after each task.

## 2026-10-25, Dromon's needs and the sandbox

**Scope.** The operator asked for a report on Dromon's
`docs/architecture/KELEUSMA_INTEGRATION.md`, at `pydromon` commit `0ca8f3a`, without
changes to the binding. The findings come from reading the published Keleusma 0.2.2
crate and from a scratch probe crate under `tmp/`, run on macOS aarch64. Nothing below
was tested against Keleusma 0.2.3.

### Needs that 0.1.0 meets

Priority 1 maps onto tasks 2 to 5 as Dromon states, and priority 2's `vm.stream` onto
task 6. Hot swap, structs, enums and natives stay outside 0.1.0, as Dromon proposes.
Two findings change what tasks 3 and 4 must do.

- **Arena sizing.** The size from `auto_arena_capacity_for` is too small after the first
  `Reset` and for a composite argument. A stream processor would fail on its second
  iteration. This is a defect in task 1's `OwnedVm`, which tasks 3 and 5 must fix.
- **Composite decoding.** A yielded tuple of scalars and literal strings arrives as flat
  bytes with no per-field types, and `TupleBody::elements()` panics on that form. Task 4
  must decode by the declared type, which the binding can read from the parsed program.
  The same route yields the fraction bits of `Fixed<N>`.

### Fit with the design

- **Many VMs on one thread.** Fact. 1,000 VMs created and resumed round-robin on one
  thread completed 4,000 of 4,000 steps once each arena was large enough. The core VM
  holds no global or thread-local state. The only thread-local is in the command-line
  shell module, which the binding does not use. The limit is memory, about 1.3 KB of
  resident memory per idle VM with a trivial script in the earlier measurement, plus
  each arena.
- **Readable worst-case figures.** Feasible. `Vm::wcet_per_iteration()` gives the worst
  case of one stream iteration, including declared native bounds, in the nominal cost
  model's unitless units. They are not cycles or seconds. Converting them needs a
  calibrated cost model, which Keleusma documents and 0.1.0 would not include. For
  memory, the arena capacity is the true figure for this runtime in bytes, while the
  module header's `wcmu_bytes` counts nominal 32-byte value slots and understates it.
  Neither figure covers Python, the binding's conversions, or the host's own work.

### Answers to Dromon's four questions

1. **Yes.** Several VMs can be created and resumed on one thread, with no limit beyond
   memory. Each must stay on the thread that created it.
2. **Yes**, as described above. Time comes in unitless nominal cost units and memory
   in bytes, and both cover only the script and declared native bounds.
3. **Mostly.** Tagged tuples work, with three constraints.
   - **Tuple size.** A tuple has two or more elements, so `("turn_end",)` is rejected,
     and `("turn_end", "")` is the substitute.
   - **One type.** Every yield of a processor shares one declared type. A fixed-arity
     tuple such as `(Text, Word, Text)` with padding fields works, and so does an array
     of tuples for several actions.
   - **Text.** A host string passed in can be compared with a literal tag, and it can be
     yielded back. Any other text work, such as searching tokens, needs a host native,
     which conflicts with Dromon's design without natives. Dromon should keep text
     inspection on the Python side.

   Keleusma enums compile and would suit actions better. Converting them is after 0.1.0.
   One caution comes from the probe. A yield of the wrong type, such as `("wake", 3)`
   where `(Text, Text)` is declared, passed both the compiler and the verifier, and only
   decoding caught it. That was reported to `v0.2.3`.
4. **Yes.** The binding converts a yielded composite into owned Python objects before it
   returns from `call` or `resume`. The tuple Dromon holds is a Python tuple, unaffected
   by the next resume. Only the VM's internal copy becomes invalid, and Keleusma reports
   a late read as a clean error rather than undefined behaviour.

### Proposed roadmap changes, for the operator's approval

1. **Task 3** sizes the arena so that a stream survives `Reset` and composite arguments.
   It is done when a stream runs 10,000 iterations in an auto-sized arena. The method
   waits on `v0.2.3`. The interim option is a documented margin that is not proven.
2. **Task 4** decodes by the declared type of the entry point, read from the parsed
   program, covering flat tuples, arrays, `Text` and `Fixed<N>`. It is done when every
   shape in the probe round trips.
3. **Task 5** adds a test of 1,000 VMs created and resumed round-robin on one thread.
4. **New task 5b** exposes `vm.wcet_per_iteration` in nominal cost units and
   `vm.arena_capacity` in bytes, documented as covering the script and declared native
   bounds only.
5. **Task 8** adds a batched update-loop example in Dromon's event and action shape.
6. **Task 7**, natives, stays in 0.1.0 after Dromon's needs, since Dromon does not need it.

### Sandbox

- **uv.** Still 0.9.5 on the `PATH`, at `~/Library/Python/3.9/bin/uv`, so the upgrade did
  not take effect.
- **sccache.** It works inside the sandbox if its server runs in the foreground inside
  the sandbox. Builds connected to it and a clean rebuild hit the cache. A server
  started outside the sandbox must not serve sandboxed builds.
- **Trust boundary.** The sandbox may now write `~/.cargo`, which holds `bin`,
  `config.toml` and `credentials.toml`, and the sccache and uv caches. Those are read or
  run later outside the sandbox, so a write there can escape it. Narrower scopes are
  `~/.cargo/registry` and `~/.cargo/git`, and separate cache directories for agent
  sessions.

**Next.** Task 2 once the operator decides, or the arena fix first if preferred.
