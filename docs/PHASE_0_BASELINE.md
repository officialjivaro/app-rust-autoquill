# Phase 0 Baseline

Measurement date: 2026-08-20
Development host: Windows x64
Python/PyInstaller comparison executable: 49,354,727 bytes

## Release measurements

| Renderer | Raw executable bytes | MiB | Smoke-test duration | Result |
|---|---:|---:|---:|---|
| FemtoVG | 8,558,592 | 8.16 | 8,220 ms warm | Passed |
| Software | 8,654,336 | 8.25 | 666–1,007 ms warm | **Selected** |

## Selection rule

Use the software renderer as the Phase 0 default. It is only 95,744 bytes larger than FemtoVG on
the Windows baseline, while repeated warm smoke runs were roughly 8–12 times faster. The full
window capture showed crisp text, correct scaling, and no software-renderer-specific artifacts.

Keep the `renderer-femtovg` feature available so this choice can be revisited as the interface gains
animation and more complex visual effects.

## Notes

- Measurements use the `release-size` Cargo profile.
- Raw executable size excludes installers, symbols, DMGs, and AppImage compatibility wrappers.
- The smoke-test duration includes a deliberate 100 ms event-loop window before clean shutdown.
- Startup timings are host-specific and include Windows process, graphics, and security overhead.
- The first FemtoVG cold run measured 10,204 ms; the table records its faster warm repeat.
- Final software-renderer warm repeats measured 1,007 ms and 666 ms.
- Phase 0 is 83% smaller than the 49,354,727-byte Python/PyInstaller comparison executable.
