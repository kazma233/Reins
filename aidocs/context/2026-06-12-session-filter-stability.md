# Session Filter Stability

## Context

Session filtering is backed by a list request. Clearing the list or selected detail while the request is in flight makes the UI flash: the sidebar briefly switches to loading or empty content, and the detail panel can disappear if the selected session is not part of the filtered result.

## Current Decision

- Keep the existing list visible while a filter request is loading.
- Show a lightweight inline loading status instead of a full panel overlay.
- Keep the selected session detail loaded even if the current filtered list no longer contains it.
- Reset list scroll only after the new list result is applied.

## Constraint

Changing the source app can still clear the selected session, because it is a different data scope. Filtering within the same source should not clear the detail panel by itself.
