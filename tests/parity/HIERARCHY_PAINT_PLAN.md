# Hierarchy-dependent paint/layout unification plan

## Root cause

ironpress has multiple container-specific child layout and paint walks. The normal block/container path can produce and paint many `LayoutElement` variants, but table cells, simple flex items, grid items, inline-blocks, and nested table/flex renderers each select their own subset of child types. When a child is represented as text, the result is often correct. When the same child is represented as a replaced element, SVG, nested table, generated-content wrapper, positioned descendant, or visual decoration, the per-container path may never collect it or may collect it and then drop it during nested painting.

The known `img`-in-`td` failure is one instance of this pattern. The same architectural split also drops inline SVG, nested tables, gradients, shadows, generated content, absolute descendants, simple flex-item replaced children, and inline-block image descendants in the confirmed probes below.

## Divergence map

### Layout collection

- Normal route dispatch is centralized in `src/layout/engine.rs:5973-6105`: flex, grid, multicol, and block containers are sent to separate layout modules. Replaced/image and inline SVG are special-cased before normal child flow in `src/layout/engine.rs:4312-4374`; form/progress layout is separate at `src/layout/engine.rs:4378-4410` and `src/layout/engine.rs:4719-4770`.
- Normal block layout can choose a wrapper `Container` path and recurse through `flatten_element` for block/SVG/display-block children in `src/layout/block.rs:2184-2304`, then emits a full `LayoutElement::Container` in `src/layout/block.rs:2626-2701`. Earlier block fast paths use their own filtering, for example `has_block_kids_for_wrapper` in `src/layout/block.rs:790-798` and recursive child filtering in `src/layout/block.rs:1318-1376`.
- Table cells use `collect_table_cell_content_inner` in `src/layout/table.rs:3331-3577`, not the normal block child routine. It special-cases nested tables in `src/layout/table.rs:3484-3500`, SVG/empty visual blocks in `src/layout/table.rs:3501-3512`, then otherwise recursively collects only text-like descendants in `src/layout/table.rs:3548-3572`. Direct non-text children outside that list are easy to miss.
- Simple flex items are split by `item_has_block_children` in `src/layout/flex.rs:1468-1471`. Items with block children use `flatten_element` in `src/layout/flex.rs:1534-1579`; otherwise they use `FlexTextRunCollector` in `src/layout/flex.rs:1702-1715`, which recurses text but does not synthesize inline boxes for `img`/`svg`.
- Grid item text is collected by `collect_grid_item_runs` in `src/layout/grid.rs:1036-1049`. Block-like grid children are separately flattened in `layout_grid_item_children` in `src/layout/grid.rs:1263-1465`; inline replaced direct children are not part of that nested child list.
- Inline atomic containers are split in `src/layout/inline.rs:305-450`: inline flex/grid/table call their container layout routines, but ordinary inline-blocks later use `FlexTextRunCollector` in `src/layout/inline.rs:1119-1131` and store `nested_elements: Vec::new()` in `src/layout/inline.rs:1228-1258`. This drops direct replaced descendants inside ordinary inline-blocks.
- Multicol is closer to the desired model: it flattens children through `flatten_element` in `src/layout/multicol.rs:246-280` and emits wrapper/fragment containers that later use the container renderer.

### Paint recursion

- Top-level table and grid rows call `render_cell_content` from `src/render/pdf.rs:5506` and `src/render/pdf.rs:5669`. That function enters the table-cell nested renderer in `src/render/pdf/layout_elements.rs:302-332`.
- `render_nested_layout_elements` in `src/render/pdf/layout_elements.rs:806-1117` only paints `TableRow`, `TextBlock`, and `Container`; the final `_ => {}` at `src/render/pdf/layout_elements.rs:1116` drops `Image`, `Svg`, `FlexRow`, `GridRow`, `ProgressBar`, `MathBlock`, and other child variants. Its planner has the same subset in `src/render/pdf/layout_elements.rs:1130-1240`.
- The nested text-block painter handles solid color, SVG background, blur, borders, and text in `src/render/pdf/layout_elements.rs:508-804`, but the `TextBlock` match explicitly ignores linear/radial/conic gradients at `src/render/pdf/layout_elements.rs:973-975`. It also has no box-shadow/outline/transform/opacity group support.
- `render_container_children` is the closest existing unified renderer. It starts at `src/render/pdf.rs:10989` and handles normal flow ordering, floats, abspos, clipping, opacity/mix-blend, masks, gradients, shadows, backgrounds, nested tables, images, SVGs, flex rows, and overflow. Examples: gradients/shadows/clip wrappers in `src/render/pdf.rs:12300-12949`, `Image` at `src/render/pdf.rs:13008-13143`, `Svg` at `src/render/pdf.rs:13144-13215`, and nested `FlexRow` at `src/render/pdf.rs:13216-13743`.
- The top-level flex renderer has another manual nested switch at `src/render/pdf.rs:6876-7444`. It handles some nested `TextBlock`/`TableRow`/`Svg`/`Container`/`FlexRow` variants, delegates some container children to `render_container_children`, then drops the rest through `_ => {}` at `src/render/pdf.rs:7444`.
- Nested table rows inside containers are also special-cased in `render_nested_table_rows` at `src/render/pdf.rs:13793-14348`; its table branch paints cell text directly in `src/render/pdf.rs:13988-14062`, while its grid branch delegates nested rows back through `render_container_children` in `src/render/pdf.rs:14285-14337`.

## Empirical probe matrix

Legend: `OK` = matches normal-flow/Chrome expectation, `BUG` = hierarchy-specific drop or wrong paint, `BASE` = unsupported already in normal flow, `SKIP` = excluded known exemplar.

| Container context | Confirmed OK | Confirmed BUG | Notes |
| --- | --- | --- | --- |
| Normal block flow | raster `img`, SVG-as-img, inline `svg`, nested table, linear-gradient block, box-shadow, overflow clip, generated `::before`, absolute descendant, positioned/float/multicol image controls | `input`/`progress` are `BASE` in the probe | Used as the control for fixture pages. |
| Table `td` / `th` / `thead` cell | own cell background/border/text; isolated overflow clip child | inline `svg`, nested table, gradient block, box-shadow block, generated `::before`, absolute descendant; direct `img` is `SKIP` because it is the known exemplar | Extra `/tmp` probe showed the gradient drop for `td`, `th`, `thead`, and `caption`. |
| Table `caption` | caption/table own border/background | gradient child, inline `svg` child | Same nested table-cell style renderer/collector symptom. |
| Inline-table | wrapper participates as an inline atomic table | cell descendants inherit the same table-cell `BUG` set | Source path uses `flatten_table` from `src/layout/inline.rs:426-440`. |
| Grid item | direct raster `img`, inline `svg`, gradient block, box-shadow block, overflow clip, absolute descendant, nested table | form controls are `BASE` | Grid item nested rows currently route through `render_container_children` for many block children. |
| Simple flex item | block visual child, gradient, box-shadow, overflow clip, absolute descendant, nested table | direct raster `img`, direct inline `svg`; form controls are `BASE` | Simple item text collector drops replaced/vector children before paint. |
| Inline-block | border/background and normal text | direct raster `img` descendant | Ordinary inline-block path collects text only and stores no nested elements. |
| Positioned / relative / fixed box | image and absolute-descendant controls OK in block path | no confirmed hierarchy bug in this probe set | The shared container renderer handles positioned descendants when layout produced them. |
| Float | image control OK | no confirmed hierarchy bug in this probe set | Floats go through `render_container_children` ordering/placement. |
| Multicol | direct image plus text OK | no confirmed hierarchy bug in this probe set | Multicol uses normal flattening before fragment containers. |
| Overflow hidden/scroll box | clipped block child OK in normal/container path and in the isolated table-cell clip control | no fixture-worthy hierarchy bug confirmed in this pass | Top-level/container path clips; table-cell clip was not carried as a regression after isolated confirmation. |
| List item / `::marker` | source supports image markers as inline boxes in the main text renderer | no new confirmed hierarchy bug in this probe set | Not fixture-worthy from this pass. |

## Fixtures added

These are intentionally tagged `expected_support: "unsupported"` until the unified fix lands.

- `tables-cell-inline-svg-child`: inline SVG paints in normal flow, disappears in a table cell.
- `tables-cell-nested-table-child`: nested table paints in normal flow, disappears in a table cell.
- `tables-cell-gradient-child`: linear-gradient block paints in normal flow, disappears in a table cell.
- `tables-cell-box-shadow-child`: box-shadow paints in normal flow, disappears in a table cell.
- `tables-cell-generated-before-child`: block `::before` generated content paints in normal flow, disappears in a table cell.
- `tables-cell-absolute-descendant`: positioned descendant paints in normal flow, disappears in a table cell.
- `flexbox-item-raster-img-child`: raster `img` paints in normal flow, disappears as a direct child of a simple flex item.
- `flexbox-item-inline-svg-child`: inline SVG paints in normal flow, disappears as a direct child of a simple flex item.
- `img-inline-block-descendant`: raster `img` paints in normal flow, disappears as a direct child of an inline-block.

## Proposed unification

Introduce one recursive child layout and paint contract:

- Layout side: every container asks a shared child routine to classify children into inline runs, atomic inline boxes, replaced elements, block containers, tables, generated content, and out-of-flow positioned descendants. Container-specific code may still compute available size, alignment, fragmentation, and track/cell/flex geometry, but it must not decide which child element kinds exist.
- Paint side: every container delegates descendants to a single recursive `paint_layout_elements`/`render_box_children` routine with a frame containing origin, available width, containing-block origins, clipping, stacking context, opacity/blend group, and page resource sinks. Table cells, grid items, flex cells, inline-blocks, multicol fragments, floats, and abspos boxes should all enter this routine for descendants.
- Keep container-owned decoration separate from child recursion. A table cell can paint its own background/border using table border-collapse rules, then call the shared child painter for content. A flex item can paint its alignment-sized item box, then call the shared child painter at the item content origin.
- Delete or reduce the special nested painters after delegation is stable: `render_nested_layout_elements`, `render_nested_table_rows`, and the flex-cell manual switch should become thin adapters or disappear.

## Phased rollout

1. Add shared paint frame adapters without behavior changes.
   - Files: `src/render/pdf.rs`, `src/render/pdf/layout_elements.rs`.
   - Extract the child-iteration/order part of `render_container_children` behind a smaller API that can be called with a cell/item content origin.
   - Gate: full parity test; no fixture tags changed.

2. Unify table/grid cell descendant painting first.
   - Files: `src/render/pdf/layout_elements.rs`, `src/render/pdf.rs`.
   - Change `render_cell_content` so nested cell descendants delegate to the shared child painter instead of `render_nested_layout_elements`.
   - Keep table cell own background/border and vertical-align calculations intact.
- Expected flips: the six new `tables-cell-*` fixtures should move from FAIL/unsupported to PASS/implemented.
   - Gate: full parity test and focused table categories; check no PASS->FAIL regressions in existing table fixtures.

3. Unify table-cell child collection.
   - Files: `src/layout/table.rs`, possibly `src/layout/helpers.rs`.
   - Replace the `collect_table_cell_content_inner` child whitelist with the same child classification used by normal block layout. This phase is where the known `img`-in-`td` exemplar and any direct replaced/form descendants should be fixed systematically.
   - Gate: all table-cell child fixtures plus the separate known `img`-in-`td` regression should pass together, not by adding type-specific paint arms.

4. Unify simple flex item and inline-block child layout.
   - Files: `src/layout/flex.rs`, `src/layout/inline.rs`, `src/layout/text.rs`.
   - Replace `FlexTextRunCollector` use for simple flex items and ordinary inline-blocks with the shared inline/block child classifier, so direct `img`/`svg` become atomic inline boxes or nested layout elements consistently.
   - Expected flips: `flexbox-item-raster-img-child`, `flexbox-item-inline-svg-child`, and `img-inline-block-descendant`.
   - Gate: flexbox and inline/images categories first, then full parity.

5. Remove remaining duplicate paint switches.
   - Files: `src/render/pdf.rs`, `src/render/pdf/layout_elements.rs`.
   - Convert the top-level flex nested switch at `src/render/pdf.rs:6876-7444` and `render_nested_table_rows` at `src/render/pdf.rs:13793-14348` to delegate to the same child painter. Keep only container-specific decoration and geometry.
   - Gate: full parity, with special attention to z-index, relative/flex paint order, border-collapse, overflow clipping, and opacity/blend fixtures.

## Risks and guards

- Paint order and stacking contexts can regress when moving children to a unified traversal. Guard with existing z-index, flex paint-order, opacity, mix-blend, and positioned-descendant parity cases.
- Table row height and vertical-align can regress if child height estimation changes while fixing collection. Guard with table baseline, rowspan/colspan, vertical-align, and the new nested-child table fixtures.
- Clipping and border-radius can regress because table cells and containers clip at different boxes. Guard with existing overflow/border-radius fixtures before and after any shared-painter migration.
- Absolute containing-block origins can shift when static intermediate containers are bypassed. Guard with existing abspos fixtures plus `tables-cell-absolute-descendant`.
- The first unified renderer phase may expose currently hidden unsupported children. Keep newly confirmed gaps tagged `unsupported` until their implementation phase lands, and rely on the PASS->FAIL parity gate to catch regressions in already implemented fixtures.
