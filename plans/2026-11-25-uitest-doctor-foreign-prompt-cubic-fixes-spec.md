# uitest doctor foreign-prompt: Cubic review fixes (PR #62)

Adopted in-flight. PR #62 (`feat/uitest-doctor-foreign-prompt`) adds a Doctor check that fails while a foreign keychain prompt is open. Scope: address Cubic's review on head d2bb068. Do not change behaviour beyond these.

1. `tools/uitest/Sources/uitest/main.swift:~259`: skip SecurityAgent windows with empty label text before the `isForeign` test (`isForeign("")` is true, which gives a false failure with an empty fix).
2. `tools/uitest/Sources/UITestCore/Doctor.swift:~91`: normalise embedded newlines (collapse to single spaces) in prompt text before inserting it in the fix, so one MISSING check stays one line.
3. `DoctorTests.swift:~96`: `foreignPromptFailsDoctor` uses a prompt longer than 120 chars and asserts the exact line `MISSING Foreign keychain prompt: answer or dismiss it: <first 120 chars>`.
4. `DoctorTests.swift:~127`: assert `report.checks` has no `Foreign keychain prompt` entry when Accessibility is unavailable (skipped, not failing).
5. Add tests for 1 and 2 (empty-text window skipped; multi-line text yields a single line).
6. Verify: `swift test --package-path tools/uitest`. Push, reply in each Cubic thread with the `<!-- cwf:agent-reply -->` marker and resolve it.
