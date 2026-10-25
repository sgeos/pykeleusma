---
name: Bug report
about: Report a defect in the Python binding
title: ''
labels: bug
assignees: ''
---

<!--
A crash of the Python process, or a module admitted that verification should reject,
may be a vulnerability. Please report it privately, as SECURITY.md describes, rather
than here.
-->

## Summary

<!-- One or two sentences. State the expected behaviour and the observed behaviour. -->

## Versions

- pykeleusma (`python -c "import keleusma; print(keleusma.__version__)"`):
- Keleusma (`keleusma.KELEUSMA_VERSION`):
- Python (`python -VV`), and whether it is free-threaded:
- Operating system and processor:
- Installed from a wheel or built from the source distribution:

## Minimal reproducer

```python
import keleusma

module = keleusma.compile("fn main() -> Word { 42 }")
print(keleusma.VM(module).call())
```

## Observed behaviour

<!-- Include the full exception and traceback. -->

## Keleusma or the binding

<!--
If the same script misbehaves in Keleusma itself, the defect may belong to the Keleusma
repository. The README lists known Keleusma 0.2.2 limitations under Limitations. Say
here whether you checked either.
-->
