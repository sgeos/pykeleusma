# Security Policy

## Supported versions

Only the newest release receives fixes. The project is in early development, and a fix
ships as a new release rather than as a patch to an older one.

## Reporting a vulnerability

Please report a vulnerability privately through the repository's
[private vulnerability reporting](https://github.com/sgeos/pykeleusma/security/advisories/new),
not in a public issue. A report that includes a script, the Python and pykeleusma
versions, and the observed and expected behaviour can be reproduced fastest.

A defect in Keleusma itself, rather than in this binding, belongs to the
[Keleusma repository](https://github.com/sgeos/keleusma). If the boundary is unclear,
report it here, and it will be forwarded.

## What the binding does and does not protect

The README section "What is guaranteed, and what is not" states the trust boundary.
In brief, Keleusma's bounds cover the script and the bounds declared for each native,
and never Python code. A Python native, a Python object carried as an opaque value, and
the host itself run outside every bound.

Treat precompiled bytecode as code. Load it only from a source as trusted as the
script's own source, or require a signature from a key held by such a source. A
signature identifies who produced the bytes and does not make their contents safe.

The following are vulnerabilities in scope for this repository.

- A crash of the Python process, such as a segmentation fault, reached from Python
  code that uses only the public interface, including through malformed bytecode.
- A module that verification rejects being admitted, or a signed module accepted
  without a valid signature from a registered key.
- A panic in the extension crossing into Python other than through PyO3's own thread
  check, which the design document records.
