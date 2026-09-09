# UI Flattening Direction

## Decision

Nyleen uses a flat product-workbench interface inspired by Coinbase marketing surfaces:

- White canvas is the default workspace surface.
- Soft gray is reserved for the left rail, code blocks, and low-emphasis controls.
- Coinbase Blue (`#0052ff`) is the only action/accent color for primary actions and active navigation.
- Cards are not nested by default. Prefer section dividers, row separators, and whitespace.
- Dialogs and transient overlays may keep one rounded container because they need modal separation.

## Component Rules

- Page shells define structure; inner content should not add another bordered shell.
- List items use bottom hairlines instead of boxed cards.
- Primary/active controls use compact 8px-radius geometry and blue fill.
- Secondary controls use compact 8px-radius geometry with soft-gray surfaces.
- Empty states are plain text or light inline hints, not dashed boxes.
- Status colors are text-only where possible; avoid colored backgrounds except very light semantic hints.
- Different target scopes should be separated structurally. Project targets and global targets use separate scope panels with explicit scope labels instead of two identical stacked sections.

## Rationale

The previous UI stacked page containers, panels, list cards, and empty-state boxes, making the app feel heavier than the underlying workflows. The new direction keeps the management UI dense and legible while matching the requested Coinbase-like restraint.
