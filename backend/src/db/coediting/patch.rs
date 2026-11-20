use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Splice {
    pub(super) range: Range<usize>,
    pub(super) text: String,
}

#[derive(Debug)]
pub(super) struct Conflict {
    pub(super) range: Range<usize>,
    pub(super) mine: String,
}

impl Splice {
    pub(super) fn apply(&self, source: &str) -> Option<String> {
        let bytes = byte_range(source, &self.range)?;
        let mut updated = String::with_capacity(source.len() - bytes.len() + self.text.len());
        updated.push_str(&source[..bytes.start]);
        updated.push_str(&self.text);
        updated.push_str(&source[bytes.end..]);
        Some(updated)
    }
}

pub(super) fn fragment<'a>(source: &'a str, range: &Range<usize>) -> Option<&'a str> {
    source.get(byte_range(source, range)?)
}

fn byte_range(source: &str, range: &Range<usize>) -> Option<Range<usize>> {
    if range.end < range.start {
        return None;
    }
    let mut offsets = source
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(source.len()));
    let start = offsets.nth(range.start)?;
    let end = if range.is_empty() {
        start
    } else {
        offsets.nth(range.len() - 1)?
    };
    Some(start..end)
}

pub(super) fn difference(base: &str, changed: &str) -> Splice {
    let prefix = base
        .chars()
        .zip(changed.chars())
        .take_while(|(left, right)| left == right)
        .count();
    let base_size = base.chars().count();
    let changed_size = changed.chars().count();
    let suffix_limit = base_size.min(changed_size) - prefix;
    let suffix = base
        .chars()
        .rev()
        .zip(changed.chars().rev())
        .take(suffix_limit)
        .take_while(|(left, right)| left == right)
        .count();
    let replaced = prefix..changed_size - suffix;
    Splice {
        range: prefix..base_size - suffix,
        text: fragment(changed, &replaced)
            .expect("range follows character counts")
            .to_owned(),
    }
}

fn shifted(index: usize, from: &Range<usize>, inserted: usize) -> usize {
    index - from.len() + inserted
}

// An insertion at a shared gap follows the earlier acknowledged insertion.
// Boundary insertions survive a replacement.
fn translate(
    incoming: &Range<usize>,
    accepted: &Range<usize>,
    inserted: usize,
) -> Option<Range<usize>> {
    if accepted.end <= incoming.start {
        Some(shifted(incoming.start, accepted, inserted)..shifted(incoming.end, accepted, inserted))
    } else if incoming.end <= accepted.start {
        Some(incoming.clone())
    } else {
        None
    }
}

pub(super) fn project(
    incoming: Splice,
    accepted: &Splice,
    before: &str,
) -> Result<Splice, Conflict> {
    let inserted = accepted.text.chars().count();
    match translate(&incoming.range, &accepted.range, inserted) {
        Some(range) => Ok(Splice { range, ..incoming }),
        None => Err(expand(&incoming.range, incoming.text, accepted, before)),
    }
}

pub(super) fn disjoint_conflict_range(
    conflict: &Conflict,
    accepted: &Range<usize>,
    inserted: usize,
) -> Option<Range<usize>> {
    // An unresolved insertion at a gap still owns that gap until merged.
    if conflict.range.is_empty() && conflict.range == *accepted {
        return None;
    }
    translate(&conflict.range, accepted, inserted)
}

pub(super) fn project_conflict(conflict: Conflict, accepted: &Splice, before: &str) -> Conflict {
    let inserted = accepted.text.chars().count();
    if let Some(range) = translate(&conflict.range, &accepted.range, inserted) {
        Conflict { range, ..conflict }
    } else {
        expand(&conflict.range, conflict.mine, accepted, before)
    }
}

fn expand(local: &Range<usize>, mine: String, accepted: &Splice, before: &str) -> Conflict {
    let start = local.start.min(accepted.range.start);
    let end = local.end.max(accepted.range.end);
    let left = fragment(before, &(start..local.start)).expect("source-aligned conflict prefix");
    let right = fragment(before, &(local.end..end)).expect("source-aligned conflict suffix");
    Conflict {
        range: start..shifted(end, &accepted.range, accepted.text.chars().count()),
        mine: format!("{left}{mine}{right}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splice(start: usize, end: usize, text: &str) -> Splice {
        Splice {
            range: start..end,
            text: text.to_owned(),
        }
    }

    #[test]
    fn maps_both_insertions_at_the_same_gap_after_multibyte_characters() {
        let before = "🌐е́ю";
        let earlier = splice(2, 2, "🐈");
        let later = project(splice(2, 2, "β"), &earlier, before).unwrap();
        assert_eq!(later, splice(3, 3, "β"));
        assert_eq!(
            later.apply(&earlier.apply(before).unwrap()).unwrap(),
            "🌐е🐈β́ю"
        );
    }

    #[test]
    fn insertion_at_a_replacement_boundary_is_preserved() {
        let before = "abcd";
        let earlier = splice(1, 1, "X");
        let later = project(splice(1, 3, "Y"), &earlier, before).unwrap();
        assert_eq!(
            later.apply(&earlier.apply(before).unwrap()).unwrap(),
            "aXYd"
        );
    }

    #[test]
    fn overlapping_changes_keep_fragments_while_unrelated_changes_shift_them() {
        let before = "abc123xyz";
        let earlier = splice(3, 5, "Q");
        let overlap = project(splice(4, 6, "R"), &earlier, before).unwrap_err();
        let after = earlier.apply(before).unwrap();
        assert_eq!(overlap.mine, "1R");
        assert_eq!(fragment(&after, &overlap.range), Some("Q3"));
        let appended = splice(0, 0, "+");
        let shifted = project_conflict(overlap, &appended, &after);
        assert_eq!(
            fragment(&appended.apply(&after).unwrap(), &shifted.range),
            Some("Q3")
        );
    }

    #[test]
    fn codecs_reject_out_of_bounds_and_preserve_exact_bytes() {
        let original = "ёж\u{301}\nλ";
        let current = "ёж🙂\nλ!";
        let transition = difference(original, current);
        assert_eq!(transition.apply(original).as_deref(), Some(current));
        assert!(splice(10, 11, "").apply(original).is_none());
        assert!(splice(4, 2, "").apply(original).is_none());
    }
}
