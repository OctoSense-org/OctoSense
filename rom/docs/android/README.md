# Android performance and launcher records

Engineering records for the phone shell on the OnePlus 6 bench (15–16 September 2026). Raw runs, traces, APKs and helper scripts they cite live in the bench machine's `target/perf-artifacts/` (ignored by git); the records carry the hashes and per-run tables.

- [perf-gap-analysis.md](perf-gap-analysis.md) — where the frames go: per-scenario diagnosis, ranked costs, the kgsl GPU/clock measurements, why the Vulkan build is slower, and the two candidate rounds that landed (scene cache, flat materials, sheet capture, deferred app capture, fade overlay).
- [performance-plan.md](performance-plan.md) — the target, the measurement contract, dated status paragraphs.
- [launcher-plan.md](launcher-plan.md) — the Home-role decision, what public APIs do and do not give, the Phase 4 (privileged) probe, the home-page pulls.
- [perf-findings-oneplus-6t.md](perf-findings-oneplus-6t.md) — the earlier OnePlus 6T / Android 11 findings; keep its numbers apart from the OnePlus 6 ones.
- [validation-record.md](validation-record.md) — the bench's validation log: exact patches and APK hashes, run blocks, rejections.
- [vulkan-probe-record.md](vulkan-probe-record.md) — the unchanged Vulkan backend on the same phone, with its kgsl trace.
