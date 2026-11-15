# kn-viewer

CLI and headless reports for re-konung-viewer. Window assembly lives in kn-fe-viewer.

- CLI parsing and initial map resolution; invalid selections return failure.
- check builds scenes/decodes textures without a window or GPU and emits text/JSON.
- simulate runs the game simulation headlessly and reports failures/counters.
- ViewerConfig crosses the frontend boundary; CLI types stay here.

Details: [topic documentation](../../../docs/guides/viewer.md).
Checks: `just cargo-test`, `just cargo-lint`. Pure logic tests need no window/GPU;
real-data cases use local original/ and skip when absent.
