pub(crate) fn slice_page<T: Clone>(
    items: &[T],
    offset: usize,
    limit: usize,
) -> (Vec<T>, usize, Option<usize>, usize) {
    let total_count = items.len();
    let start = offset.min(total_count);
    let end = start.saturating_add(limit).min(total_count);
    let next_offset = (end < total_count).then_some(end);

    (items[start..end].to_vec(), start, next_offset, total_count)
}
