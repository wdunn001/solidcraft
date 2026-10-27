# CAM physics core plan

Status: phases 0 to 4 are built on `feat/cam-physics-core` (see "What was built" below). The UI
work in the last part is open. Follows the research on #1621 (nsnlookupjn, "the CAM Physics
System") and builds on #5751 (`solidcraft-cam`) and #2881 (the `cam` design field and `cam.*`
commands).

## What was built, and where it differs from the plan

- **Phase 0.** The tool's physical data is one optional `physical` block on `Tool` (corner
  radius, shank, stick-out, tool material, helix, holder, limits, catalogue reference, wear,
  runout, source). `Material` and `Machine` live in `solidcraft-cam` (`material.rs`,
  `machine.rs`); the design holds `materials` and `machines`, and a setup names its `material`
  and `machine` and sets `confidence`. An operation's `physics` holds its own confidence,
  accepted findings and `adapt_feed`. Commands: `cam.material.define`/`remove`,
  `cam.machine.define`/`remove`, `cam.tool.define` takes `physical`, `cam.setup` takes
  `material`, `machine`, `confidence`.
- **Phase 1, changed.** Engagement is measured, not reported by each generator: `Stock`
  (`engage.rs`) is a height field over the stock box, and every move of every operation is cut
  through it in order. Each move gets its radial width and axial depth at its worst point and a
  kind (side, slot, ramp, plunge, rapid). This judges contours, pockets, hand-made paths and
  read-back G-code the same way, and a new generator needs no engagement code of its own.
  Without a stock box the check uses a box around the toolpaths and says the stock is assumed.
- **Phase 2.** `limits.rs`: Kienzle forces with the side-milling mean chip thickness, spindle
  power and torque against the machine's curve, spindle and holder speed, machine feed, tool
  torque, force and deflection, rubbing, and rapids in material. `cam.check` returns the
  findings, the binding limit, peaks, and notes on what was not checked.
- **Phase 3.** `cam.post` checks the program it writes and refuses it on a stop finding.
  `cam.finding.accept {setup, operation, limit, reason}` lets one limit through on one
  operation (`revoke` takes it back); every accepted finding is written as a comment in the
  G-code header. Checks resting on an assumed input never stop.
- **Phase 4.** `adapt_feed` on an operation scales each cutting move's feed to its binding
  limit, between 0.25 and 2 times the programmed feed, never faster than programmed when the
  limit rests on an assumption, smoothed so neighbouring moves differ by at most 1.5 times.
- **Not done yet.** Input and output schemas for the `cam.*` commands (none of them have
  schemas yet, #2881's included); one follow-up should add them for all `cam.*` commands at once.

## Goal

Leave room for cutting physics in the CAM core before the format settles, so later work plugs
in without reworking generators or files:

1. The design carries the physical data: tool assemblies, material, machine, a confidence
   setting.
2. Every toolpath generator reports how much material the tool engages on each cutting move.
3. One limits engine checks every move and names the limit that set the answer.
4. Posting runs that check on the program as written and stops on a hard limit, unless the user
   accepts the finding, and the acceptance is recorded.

The `cam-milling` format is still pending (`crates/doc/src/format/pending/cam-milling.rs`), so
adding fields now costs no migration. That is why this goes first.

## What exists today

- `solidcraft-cam` (#5751): `Tool` has number, kind (flat, ball, drill), diameter, flute length,
  flutes, and optional starting spindle speed, feed, plunge feed, stepdown and stepover.
  `Toolpath` is a list of `Move` (rapid, feed, arc, dwell) with a feed on each cutting move.
  `contour` and `pocket` build paths and `check::verify` checks them. `gcode::emit` writes a
  `Program` through a data `Post`; `readback` parses the text and holds it against the paths.
- `cam` design field (#2881): `CamSettings { tools, setups, posts }`; `CamSetup` holds the work
  frame, work offset, an optional `StockBox` and the operations. Feeds and depths are
  expressions evaluated at post time. Toolpaths are not saved.
- No material, machine, holder, engagement or limits anywhere.

## Rules for all phases

- Every new field is optional, so a design without it posts exactly as today.
- Every number from a file or a library is hostile: finite, in range, capped, as the repository
  requires. No panics.
- Millimetres, radians, newtons, watts, newton metres, rev/min, mm/min.
- Each value that physics reads carries its source: `measured`, `catalogue` or `assumed`.
- Missing data never blocks. It makes the check conservative and says so.
- Tests use closed-form expected values (the repository's oracle approach), never numbers made
  by another CAM program.
- Formulas come from public sources, cited in the code comments: Sandvik Coromant's milling
  formulas, Altintas (2012), Machinery's Handbook. No copyleft source is read.

## Phase 0: data in the design (core, one PR on #2881)

Crate `solidcraft-cam`, `tool.rs`:

1. Add to `Tool`, all optional:
   - `corner_radius` (mm), `shank_diameter` (mm), `stick_out` (mm, length out of the holder).
   - `tool_material`: `hss`, `carbide`, `ceramic`, `pcd`, `cbn`, `other`.
   - `helix_angle` (rad), `rake_angle` (rad).
   - `holder: Option<Holder>` with `name`, `gauge_length` (mm), `diameter_at_nose` (mm),
     `max_rpm` (rev/min).
   - `limits: Option<ToolLimits>` with `max_torque` (N m), `max_force` (N), `max_deflection`
     (mm).
   - `catalogue: Option<CatalogueRef>` with `library`, `id`, `version`: which shared entry this
     shop tool came from.
   - `wear: Option<f64>` (0 new to 1 worn) and `runout` (mm), the shop instance values.
   - `source: Sourced` map: which of the fields above are measured, catalogue or assumed.
2. Add `ToolKind::CornerRadius` and `ToolKind::Chamfer` only if the generators can use them;
   otherwise leave the enum alone for now.
3. `Tool::validate` checks every new field (finite, positive where it must be, stick-out at most
   the overall length, runout under the radius).

New file `material.rs`:

4. `Material { name, group, kc11, mc, youngs_modulus, yield_strength, source }`.
   `group` is the ISO 513 letter (P, M, K, N, S, H). `kc11` (N/mm²) and `mc` (dimensionless) are
   the Kienzle specific cutting force values. All optional except `group`.
5. A small built-in table of one generic entry per ISO 513 group, every value marked `assumed`
   and taken from published handbook ranges, cited per row. Real values come from libraries.

New file `machine.rs`:

6. `Machine { name, max_rpm, min_rpm, spindle: Vec<SpindlePoint>, max_feed, rapid,
   axis_thrust: Option<[f64; 3]>, source }`, with `SpindlePoint { rpm, power_w, torque_nm }`
   describing the power and torque curve. Linear interpolation between points; outside the
   curve is out of range.

Design field (`crates/doc/src/cam.rs`):

7. `CamSettings` gains `materials: Vec<Material>` and `machines: Vec<Machine>`.
8. `CamSetup` gains `material: Option<String>`, `machine: Option<String>` (names in those
   lists) and `confidence: Option<f64>` (0 conservative to 1 aggressive, default 0.5).
9. Each `CamOperation` gains `confidence: Option<f64>` (override) and
   `accepted: Vec<AcceptedFinding>` (phase 3).
10. Update the pending format note and round-trip tests (`cam-milling.rs`): an old design loads,
    a design with every field round-trips, an out-of-range value is refused with a clear
    message.

Commands (`crates/engine/src/cmd/cam.rs`):

11. Extend `cam.tool.define` with the new fields.
12. New `cam.material.define`, `cam.material.remove`, `cam.machine.define`,
    `cam.machine.remove`; `cam.setup` takes `material`, `machine`, `confidence`.
13. `cam.list` returns them. Schemas in the command specs so scripts, CLI and MCP see them.

Done when the tests above pass, nothing about posting changes, `ci` green.

## Phase 1: engagement from every generator (core)

`path.rs`:

1. `Engagement { ae, ap, kind }`: radial width of cut (mm), axial depth of cut (mm), and
   `kind`: `side`, `slot`, `ramp`, `plunge`, `corner`, `unknown`.
2. `Toolpath` gains `engagement: Vec<Option<Engagement>>`, one per move, `None` for rapids and
   dwells. `serde(default)`, so `Move` and the G-code side stay unchanged.
3. `Toolpath::validate_engagement`: same length as `moves`, finite, `0 <= ae <= diameter`,
   `0 <= ap <= flute_length`.

`ops.rs`, filled where the generator knows it:

4. Pocket: the innermost ring of each level is cut first into solid material, so it is a slot
   (`ae = diameter`); later rings take `ae = stepover`; corner spurs are `corner`; `ap` is the
   level's step down; ramp moves are `ramp` with `ap` growing along the ramp.
5. Pocket inside corners: engagement grows where the path turns. Compute the engagement angle
   at each inside corner from the turn angle, the tool radius and the stepover (closed form for
   line-line and line-arc corners) and record the peak as a `corner` entry on the move into
   the corner.
6. Contour: against the setup's stock box when known (`ae` = distance from the wall to the box
   edge, capped at the diameter), else `unknown`. Finish passes with `stock_to_leave` set:
   `ae = stock_to_leave` for the pass after the first.
7. Plunges: `plunge` with `ae = diameter`, `ap = depth of the move`.

Tests:

8. Rectangle pocket: hand-computed `ae` and `ap` on every move.
9. Independent oracle: a test-only 2D raster that removes a disc along the path and measures
   the arc of contact on each move. The generator's numbers must match it within one cell.
   This is the test that catches a generator lying about engagement.
10. Property test: random rectangles and stepovers, every cutting move has an entry, nothing
    exceeds the diameter.

## Phase 2: the limits engine (core)

New module `limits.rs` in `solidcraft-cam` (plain numeric code, builds for wasm):

1. Per cutting move, from tool, material, machine, rpm, feed and engagement:
   - cutting speed `vc = pi * D * n / 1000` (m/min);
   - feed per tooth `fz = vf / (n * z)`;
   - mean chip thickness with radial chip thinning (Sandvik's side-milling formula);
   - specific cutting force `kc = kc11 * hm^(-mc)` (Kienzle);
   - power `Pc = ap * ae * vf * kc / 60e6` (kW) and torque `M = Pc * 30000 / (pi * n)` (N m);
   - tangential force per engaged tooth, and the tool tip deflection as a cantilever over the
     stick-out, `d = F * L^3 / (3 * E * I)`, with `I` from an effective fluted diameter
     (marked as an assumption in the output).
2. Checks, each giving the allowed scale on feed (1.0 = exactly at the limit):
   - spindle power and torque against the machine curve at this rpm;
   - spindle rpm against tool holder, machine min and max;
   - tool torque, force and deflection against `ToolLimits` (or a conservative default from
     diameter and material when absent);
   - chip thickness below a minimum (rubbing) is a warning, not a scale.
3. Confidence: one function maps `confidence` to a safety factor per check, never below a hard
   floor; each `assumed` input moves its check toward conservative.
4. Output `Verdict { scale, binding: Limit, severity: ok|warn|stop, inputs: Vec<(name, value,
   source)> }` per move, and `OpReport { worst, binding, move_index, summary }` per operation.
   `summary` is one plain sentence, e.g. "limited by spindle torque at 3000 rev/min: 92% of
   the curve".
5. Unknown engagement or no material: the check runs with worst-case slot engagement and says
   so in the summary.

**Tests.** each formula against a hand calculation with the published example values; a
cantilever deflection against the closed form; a spindle curve lookup at, between and outside
its points; confidence never takes a factor below its floor.

Command:

6. `cam.check { setup, operation? }` returns the reports as JSON. It changes nothing.

## Phase 3: the gate at post (core)

1. `cam.post` already parses its own G-code (`readback`). Map each read-back cutting move to its
   toolpath move (readback already pairs them) and run the limits on the read-back feeds and
   speeds, so a hand edit or a post rounding is judged as written.
2. A `stop` finding blocks the post with the operation, the move index, the G-code line number
   and the summary.
3. `AcceptedFinding { limit, move_index, reason, accepted_at }` on the operation lets the post
   through for that one finding. An accepted finding is written as a comment in the G-code
   header and listed in the post result, so it appears on the setup sheet.
4. Until a design has a machine and a material, findings are `warn` at most. The gate only
   stops on limits it can actually judge.

**Tests.** a post that would stall the spindle is refused; the same post with an accepted finding
goes through and carries the comment; a hand-edited feed in the text is caught by readback.

## Phase 4: feed per move (core, opt in)

1. Operation flag `adapt_feed: bool`. When on, each cutting move's feed is scaled to the
   binding limit times the confidence factor, capped at the programmed feed times a maximum
   (2.0) and never below a minimum (0.25), and smoothed so the feed does not jump more than a
   set ratio between neighbouring moves.
2. The post writes the changed `F` words; readback already compares feeds.

**Tests.** a pocket's slot ring comes out slower than its stepover rings; output is identical with
the flag off.

## Later phases (data slots only, no core work now)

- Workholding (`holds` on the setup, capacity per direction): after the tool library settles the
  catalogue pattern. Needs force direction per move, which phase 2 can add.
- Machine stiffness and axis thrust checks: `axis_thrust` is already in phase 0.
- Stock integrity and cut order: needs the in-process stock from simulation (#2627) and FEA
  (#1620).
- Live feedback from the machine (MTConnect #2625, SolidCraft Edge): writes measured values into
  shop instances, which phase 0 already has room for.

## What each of the other CAM PRs needs

All of these still build on the old `crates/manufacture` crate from the #1782 split. The pattern
for each is the one #2620 and #2622 already followed. Drop the `crates/manufacture` copy, read
and write through `cam.*` commands, keep the UI.

### #2629 Manufacture UI: tools (open to the contributor)

1. Rebase onto #2622.
2. Replace `solidcraft_manufacture::Tool` with `solidcraft_cam::Tool`; delete the fields the
   core does not carry, add the phase 0 fields to the tabs:
   - General: number, description, catalogue reference.
   - Cutter: kind, diameter, corner radius, flute length, flutes, tool material, helix and rake.
   - Shaft: shank diameter, stick-out.
   - Holder: holder name, gauge length, nose diameter, max rpm.
   - Cutting data: starting rpm, feed, plunge, stepdown, stepover, plus the limits.
   - Shop: wear, runout, and the measured/catalogue/assumed marker beside every value.
3. OK writes through `cam.tool.define`, Delete through `cam.tool.remove`; the dialog never
   writes the document directly.
4. Library browser reads `cam.list`; leave the "Manage" stubs out until each has a command.
5. Tests: a UI test that defines, edits and removes a tool through the dialog and checks
   `cam.list`; the icon audit; translations for every new label (`en`, `ja`, `zh`).

### Material and machine pickers (new, open to the contributor)

1. Setup window (#2622): material and machine drop-downs and the confidence slider, writing
   through `cam.setup`.
2. Small editors for a material and a machine (spindle curve as a table of points), writing
   through `cam.material.define` and `cam.machine.define`.

### Findings display (new, open to the contributor, after phase 2)

1. Beside each operation, the `OpReport` summary sentence and a colour for its severity.
2. Selecting a finding highlights its move in the viewport and its line in the G-code preview.
3. Post dialog (#2642): lists stop findings; "Accept" asks for a reason and writes an
   `AcceptedFinding` through a command.

### #2643 Manufacture UI: libraries

1. Rebase onto the recut #2629. Keep only the tool library panel and the material and machine
   panels; drop the panels for kinds the core does not have yet (templates, holders as solids).
2. Library entries are catalogue entries; "add to design" copies one into the design as a shop
   instance with `catalogue` set.

### #2628 CAM library remotes and catalogue sync

1. Recut to sync three kinds: tools, materials, machines, each a versioned JSON schema in
   `docs/formats/`.
2. Synced entries are validated with the same `validate` functions as design data before use.
3. Offline cache; a library with a bad entry loads the rest and reports the bad one.

### #2627 toolpath height-field simulation

1. Recut onto `solidcraft_cam::Toolpath` and the setup's `StockBox`.
2. Add an engagement sampler: arc of contact per move from the height field. It becomes the
   second oracle for phase 1 and the source of engagement for paths whose generator reports
   `unknown`.
3. Run the limits on simulated engagement in `cam.check` when the generator's value is unknown.

### #2624 machine preset pack

1. Turn the profiles into `Machine` entries in the machine library (phase 0 type), one file
   each, values cited to the maker's public spec sheet and marked `catalogue`.
2. Keep the controller and protocol facts (which post, which protocol) as a separate field;
   the physics side only needs the spindle curve, rpm range, feeds and thrust.

### Posts #2623 GRBL, #2835 Marlin, #2836 Repetier, #2837 Mach3, #2838 LinuxCNC, #2839 Fanuc, #2840 Haas

1. Posts in the core are data (`post.rs`). Rewrite each dialect as a JSON `Post`, written from
   the controller's public documentation, not from any existing post processor.
2. One readback test per post: emit a pocket, parse it back, `within()` holds.
3. Marlin and Repetier are printer firmware; keep them only if a dialect needs nothing the
   `Post` data cannot say, otherwise close them.

### #2626 probe cycle snippets, #2625 MTConnect, #2647 FOCAS, #2649 THINC, #2650 job registry

1. Park until phase 3 lands. They belong to the inspection and live-feedback loop.
2. When resumed: MTConnect first (open standard), reading spindle load and feed override per
   G-code line into a report that can update a tool's shop values.

### #2630 to #2641 operation and process UIs

1. Recut only the ones whose operation exists in the core: 2D contour (#2631) and 2D pocket
   (#2630).
2. Each operation dialog writes through `cam.op.contour` or `cam.op.pocket` and shows the
   operation's `OpReport` once phase 2 lands.
3. Drill, 3D, multiaxis, turning, additive, fabrication, inspection, part handling, hole
   recognition and modify stay parked until their operations exist in the core.

## Order

1. Land #5751, then #2881.
2. Phase 0 (one PR on #2881), and in parallel the #2629 recut once the phase 0 types are up.
3. Phase 1, then phase 2 with `cam.check`, then the pickers and findings display.
4. Phase 3 with the post dialog.
5. Phase 4.
6. Library sync, simulation, machine presets and posts can proceed in parallel after phase 0.
