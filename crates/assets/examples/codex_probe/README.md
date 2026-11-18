# Codex asset investigation

Standalone research tools for the production-codex design. They are not called by the overlay, do not implement UI and are not release assets.

## Read-only probe

Build/run only the assets example, using the existing local toolchain and a cached extraction request whose native schema matches the installed executable:

~~~powershell
cargo run --offline --locked -p starrupture-assets --example codex_probe -- local/path/to/request.json local/codex-research/index
~~~

The index command reads installed archives and the cached schema, then writes native asset paths, schemas and source fingerprints under ignored local storage. It does not read a running process, invoke game code, update the installed overlay, write game archives or access saves.

For a bounded selection, supply a private JSON array of native package paths as the third argument:

~~~powershell
cargo run --offline --locked -p starrupture-assets --example codex_probe -- local/path/to/request.json local/codex-research/selected local/codex-research/selection.json
~~~

The selection is limited to 128 packages. Each native package must resolve uniquely in the installed game index; supported mounts are the main game and packaged GameFeatures content. This permits read-only investigation of weapon definitions stored in feature packages without accepting arbitrary paths. Raw export evidence records failed/unsupported objects explicitly. Required selected objects must have a complete property decode and object tail before their data is promoted into the sample. Texture selections additionally decode bounded BGRA/DXT5 images and explicitly linear half-float RGBA icons to premultiplied local BGRA files. Half-float colour is converted to sRGB; alpha remains linear. GameplayTagContainer elements use native FNames. Standalone GameplayTag structs and struct map keys retain their unversioned property headers; cooked enum map keys use FNames. UE5 FieldPath payloads contain a bounded name path and serialized owner, following the [CUE4Parse format reference](https://github.com/FabianFG/CUE4Parse/blob/master/CUE4Parse/UE4/Objects/UObject/FFieldPath.cs). Export identities retain their complete outer path so identically named events in different listeners cannot be joined by leaf name.

The example reuses extraction helpers and has a research-only property reader fork. That fork adds native double-precision geometry structs and FName-valued enum-array decoding; scalar enums remain ordinal-valued. This is not a change to the production reader or a promise of general Unreal format support. Review and consolidate any required improvements before integrating a codex into the app.

## Sample catalogue

Run scripts/research/New-CodexSample.ps1 with the PrivateRoot parameter pointing to the prepared research directory. It normalizes research batches into a private JSON catalogue and a readable Markdown report. The script lists its required batch directories explicitly. Keep selection files and raw exports beside the local evidence so the same joins can be rerun.

The catalogue separates research submissions, production inputs/output, machine construction requirements and machine-upgrade tasks/submissions. Names come from native localized text, and producing-machine links follow building definition, entity config, referenced crafting trait and recipe collection. File naming and recipe suffixes do not establish a producer or machine version.

The native research example in crates/reader/examples/codex_native_probe.rs reads the selected local mode, game-state and technology-keeper class-default chain through validated player/world ownership. It captures the keeper's authored building root, native categories and the class defaults needed by the sample. It requires a current matching overlay session cache, uses query/read access only, invokes no game code and writes evidence under ignored local storage. The first explicit research run may scan for the required native classes; later runs validate cached class indices and avoid the scan. Base Core settings and the world-owned shared-quest component provide the tier-data binding and positive weapon purchase candidates. Discovery of component indices is an explicit --discover-quest-components diagnostic; later runs validate their class/allocation/owner identities and use those indices. Mutable weapon arrays, soft references and final world ownership are revalidated through uncached reads. This diagnostic is not an app startup path.

~~~powershell
cargo run --offline --locked -p starrupture-overlay-reader --example codex_native_probe -- local/codex-research/mode-defaults PATH_TO_CURRENT_OVERLAY_CACHE
~~~

Normalization requires matching native and archive executable identities. It follows entity Config.Parent links and selects only referenced exact-type crafting traits, child first. Explicit properties, including false and zero, take precedence over independently proven native defaults; unproven defaults fail. CraftingLoopDuration is retained separately from recipe BuildTime and OverwriteRecipeBuildTime.

The normal catalogue uses the selected authored root and excludes Test categories. This conservative content scope is separate from runtime AvailableBuildings, player unlocks, upgrade completion and placement conditions. Decode success or recipe collection membership does not prove those states or eligibility in another mode. Keep effective runtime rates unavailable unless independently established.

No actual extracted catalogue, schema, texture, capture or generated binary belongs in source control. Only the probe, its synthetic tests and normalization tooling belong here.

## Checks

~~~powershell
cargo clippy --offline --locked -p starrupture-assets --example codex_probe -- -D warnings
cargo test --offline --locked -p starrupture-assets --example codex_probe
~~~

Run scripts/research/Test-CodexSample.ps1 with the prepared PrivateRoot to check recipe quantities, producer/root joins, native-default precedence and selected-trait handling against private evidence. Fresh game UI observations belong in the private comparison record and must identify visible fields and coverage gaps.

Tests cover the inherited bounded-property decoder and synthetic regressions for enum-array alignment, native vector truncation and nonfinite data. Source quantity, output and producer-link comparisons must also pass against private raw evidence. These checks do not replace comparison with the game's UI.

## Selected-root production catalogue

The catalogue tools expand the sample using the selected technology keeper's authored building root. Archive filename patterns collect candidates only. Eligible producer links come from rooted non-Test building definitions, selected entity traits and native recipe collections. Development-only recipes, unrooted recipes and unlinked item prototypes remain outside the normal catalogue. Referenced raw materials, research submissions and construction materials remain item entries even when they have no retained producing recipe.

Use the prepared private root, matching cached request and built assets research example:

~~~powershell
./scripts/research/Get-CodexCatalogueEvidence.ps1 -PrivateRoot local/codex-research -Request local/path/to/request.json
./scripts/research/New-CodexCatalogue.ps1 -PrivateRoot local/codex-research
./scripts/research/Get-CodexCatalogueIcons.ps1 -PrivateRoot local/codex-research -Request local/path/to/request.json
./scripts/research/Get-CodexUnlockEvidence.ps1 -PrivateRoot local/codex-research -Request local/path/to/request.json
./scripts/research/Test-CodexCatalogue.ps1 -PrivateRoot local/codex-research
~~~

Dependency acquisition resumes from a private manifest, bounds each archive selection and stops at its package/round limits. Icon batches have separate memory bounds and retry failed or changed selections. The resulting catalogue.json and catalogue.md retain source paths, explicit exclusions and unresolved fields. The native probe supplies matching empty input/research-container defaults; omitted values are never silently converted to empty lists without that proof.

Building unlock evidence includes corporation rewards, building-collection rewards, upgrade tasks/submissions and referenced tutorial objective listeners. Tutorial joins require complete main/listener/event exports and an explicit positive availability value. Identical event names in different listener outers stay distinct. These are authored conditional routes, not proof that a flow ran or that a particular player completed it. Optional private rule-confirmations.json records player-confirmed mechanics with executable scope and provenance. Those confirmations remain distinct from native joins: sequential Base Core upgrades do not establish initial placement, and first weapon purchase does not supply a weapon-to-ammo identity mapping. The unlock collector pins complete weapon CDOs and their localized names to positive native purchase-candidate evidence, and pins Base Core tier data to its native settings reference. Raw candidates absent from that observed purchase list remain excluded; absence is not a claim of global unavailability. Runtime rates/modifiers, current ownership, displayed tier-number formatting and custom/skip-tutorial behavior remain explicit boundaries.

The full-catalogue check compares every retained recipe's quantities and base duration, every rooted building's construction bill, producer links, tutorial event ownership and required item/building names and icons. It also checks every Base Core tier bill/cooling value and every ammunition recipe against its complete weapon CDO, registered class and native ammo reference. It verifies all decoded icon headers and payload sizes. Synthetic Rust tests additionally cover GameplayTag alignment/truncation and finite half-float colour conversion. The sample's checks remain available separately. These scripts are research tools; the overlay does not invoke them.