# Dependency review: parity spec 03 (note styles and summary-model choice)

Policy: dependency choices are reviewed only for being commonly used and
released within the last 12 months (see the dependency policy in the
`plans/parity` specs).

## Crates and npm packages

None added, bumped or removed. `Cargo.lock` and `package-lock.json` are unchanged.

- Style ids (R2): `custom-<32 hex>` from SQLite's `randomblob(16)`, which
  runs on the CSPRNG SQLCipher already uses. No `uuid` or `getrandom` crate.
- Opening licence links: the existing `open_external_link` command, which
  allows only http, https and mailto URLs.

## Model data

| Model | Status |
| --- | --- |
| `gemma-2-2b-it-q4km` | Unchanged pin; still the default. Gained a description, which is UI text only. |
| Gemma 3 4B Instruct Q4_K_M | **Not added.** It needs a source at a fixed commit, a measured sha256 and size, a bench on the 7 built-in styles (spec step 2) and Patrick's approval. Until then the manifest lists one LLM, which is the fallback the spec allows. |

The Gemma 3 support in the pinned `llama-cpp-2` has not been checked. If it
needs a version bump, that bump goes through this review before the 4B pin
lands.

The code for choosing a model, switching without a relaunch, per-model
parameters and reverting a failed download is generic (R17 to R21). Adding
the 4B model needs only a manifest entry and a `llm::llama::params` arm.
