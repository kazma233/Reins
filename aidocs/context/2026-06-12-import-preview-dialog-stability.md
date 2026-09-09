# Import Preview Dialog Stability

## Context

The import preview dialog lets users switch the target app before importing a session. Switching targets previously cleared the preview immediately, so the dialog body collapsed from a full preview panel to one line of loading text and then expanded again when the next preview returned.

## Current Decision

- Keep the previous preview rendered while a new target preview is loading.
- Track which `SourceApp` the current preview belongs to.
- Disable import until the preview belongs to the currently selected target app and reports `supported`.
- Render loading and error states inside the same minimum-height preview region to avoid modal height jumps.

## Constraint

The previous preview is only a visual placeholder during target switching. It must not enable import for the newly selected target.
