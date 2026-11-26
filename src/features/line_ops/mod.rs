// Line operations of Edit > Lines (sort, remove duplicates, join, trim).
//
// Pure text transforms over a block of lines without their line breaks.
// The editor (`ui::editor::line_ops`) picks the lines, calls one of these
// and puts the result back as one undo step. Nothing here is stored.

use std::cmp::Ordering;
use std::collections::HashSet;

/// One command of the Edit > Lines submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOp {
    SortAscending,
    SortDescending,
    RemoveDuplicates,
    Join,
    TrimTrailing,
}

/// Sort `lines` with `cmp` (the user's locale in the app), keeping equal
/// lines in their order (stable) in both directions. The first comparison
/// error stops the sort and is returned: the caller leaves the text alone.
pub fn sort_lines<E>(
    lines: &[&str],
    descending: bool,
    cmp: impl Fn(&str, &str) -> Result<Ordering, E>,
) -> Result<Vec<String>, E> {
    let sorted = merge_sort(lines, |a, b| if descending { cmp(b, a) } else { cmp(a, b) })?;
    Ok(sorted.into_iter().map(str::to_string).collect())
}

/// Stable bottom-up merge sort with a fallible comparison, O(n log n). No
/// slice::sort_by: it may panic on a comparator that stops being a total
/// order (Rust 1.81+), and release builds abort on panic. The first error
/// stops the sort and is returned.
fn merge_sort<'a, E>(
    items: &[&'a str],
    cmp: impl Fn(&str, &str) -> Result<Ordering, E>,
) -> Result<Vec<&'a str>, E> {
    let n = items.len();
    let mut src: Vec<&str> = items.to_vec();
    let mut dst: Vec<&str> = items.to_vec();
    let mut width = 1;
    while width < n {
        let mut lo = 0;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut i, mut j) = (lo, mid);
            for slot in dst[lo..hi].iter_mut() {
                // The right run wins only when strictly smaller: stable.
                let take_right = i >= mid || (j < hi && cmp(src[j], src[i])? == Ordering::Less);
                if take_right {
                    *slot = src[j];
                    j += 1;
                } else {
                    *slot = src[i];
                    i += 1;
                }
            }
            lo = hi;
        }
        std::mem::swap(&mut src, &mut dst);
        width *= 2;
    }
    Ok(src)
}

/// Drop every line that already appeared above (exact match, case and
/// whitespace included); the first occurrence stays where it was.
pub fn remove_duplicate_lines(lines: &[&str]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::with_capacity(lines.len());
    lines
        .iter()
        .filter(|l| seen.insert(l))
        .map(|l| l.to_string())
        .collect()
}

/// Join `lines` into one: the first line as it is, then each following
/// line without its leading whitespace, after one space.
pub fn join_lines(lines: &[&str]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            out.push_str(line);
        } else {
            out.push(' ');
            out.push_str(line.trim_start());
        }
    }
    out
}

/// Remove the whitespace at the end of every line.
pub fn trim_trailing_whitespace(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.trim_end().to_string()).collect()
}

/// Apply `op` to `lines`. `cmp` is only called by the two sorts.
pub fn apply<E>(
    op: LineOp,
    lines: &[&str],
    cmp: impl Fn(&str, &str) -> Result<Ordering, E>,
) -> Result<Vec<String>, E> {
    match op {
        LineOp::SortAscending => sort_lines(lines, false, cmp),
        LineOp::SortDescending => sort_lines(lines, true, cmp),
        LineOp::RemoveDuplicates => Ok(remove_duplicate_lines(lines)),
        LineOp::Join => Ok(vec![join_lines(lines)]),
        LineOp::TrimTrailing => Ok(trim_trailing_whitespace(lines)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Case-insensitive comparison standing in for CompareStringEx.
    fn ci(a: &str, b: &str) -> Result<Ordering, String> {
        Ok(a.to_lowercase().cmp(&b.to_lowercase()))
    }

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sort_ascending_ignores_case_and_is_stable() {
        let lines = ["b", "A", "a", "B", "c"];
        assert_eq!(sort_lines(&lines, false, ci), Ok(owned(&["A", "a", "b", "B", "c"])));
    }

    #[test]
    fn sort_descending_keeps_equal_lines_in_their_order() {
        let lines = ["b", "A", "a", "B", "c"];
        assert_eq!(sort_lines(&lines, true, ci), Ok(owned(&["c", "b", "B", "A", "a"])));
    }

    #[test]
    fn sort_of_empty_and_single_line() {
        assert_eq!(sort_lines(&[], false, ci), Ok(Vec::new()));
        assert_eq!(sort_lines(&["x"], true, ci), Ok(owned(&["x"])));
    }

    #[test]
    fn sort_keeps_empty_lines_and_sorts_them_first() {
        assert_eq!(sort_lines(&["b", "", "a"], false, ci), Ok(owned(&["", "a", "b"])));
    }

    #[test]
    fn sort_returns_the_comparison_error() {
        let failing = |_: &str, _: &str| -> Result<Ordering, String> { Err("boom".into()) };
        assert_eq!(sort_lines(&["b", "a"], false, failing), Err("boom".to_string()));
    }

    #[test]
    fn sort_stops_on_a_late_comparison_error_without_panicking() {
        // 200 distinct lines; the comparison fails from its 301st call on.
        let owned_lines: Vec<String> = (0..200).map(|i| format!("{:03}", (i * 7) % 200)).collect();
        let lines: Vec<&str> = owned_lines.iter().map(String::as_str).collect();
        let calls = std::cell::Cell::new(0usize);
        let flaky = |a: &str, b: &str| -> Result<Ordering, String> {
            calls.set(calls.get() + 1);
            if calls.get() > 300 { Err("boom".into()) } else { Ok(a.cmp(b)) }
        };
        assert_eq!(sort_lines(&lines, false, flaky), Err("boom".to_string()));
        // The sort stopped at the first error.
        assert_eq!(calls.get(), 301);
    }

    #[test]
    fn sort_of_many_lines_is_ordered_and_stable() {
        // Keys 0..9 repeated, tagged with their original position.
        let owned_lines: Vec<String> = (0..500).map(|i| format!("{}-{:03}", (i * 7) % 10, i)).collect();
        let lines: Vec<&str> = owned_lines.iter().map(String::as_str).collect();
        let key = |a: &str, b: &str| -> Result<Ordering, String> { Ok(a[..1].cmp(&b[..1])) };
        let mut expected = owned_lines.clone();
        expected.sort_by(|a, b| a[..1].cmp(&b[..1]));
        assert_eq!(sort_lines(&lines, false, key), Ok(expected.clone()));
        let mut expected_desc = owned_lines.clone();
        expected_desc.sort_by(|a, b| b[..1].cmp(&a[..1]));
        assert_eq!(sort_lines(&lines, true, key), Ok(expected_desc));
    }

    #[test]
    fn sort_does_not_use_the_std_sort_with_a_fallible_comparison() {
        // slice::sort_by may panic on a comparator that stops being a total
        // order (Rust 1.81+), and release builds abort on panic.
        let src = crate::read_source(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/features/line_ops/mod.rs"),
        );
        let code = &src[..src.find("#[cfg(test)]").expect("test module")];
        assert!(!code.contains(".sort_by("), "sort_lines must not call sort_by");
    }

    #[test]
    fn remove_duplicates_keeps_the_first_occurrence() {
        let lines = ["a", "b", "a", "A", "b ", "b", ""];
        assert_eq!(remove_duplicate_lines(&lines), owned(&["a", "b", "A", "b ", ""]));
    }

    #[test]
    fn remove_duplicates_of_repeated_empty_lines() {
        assert_eq!(remove_duplicate_lines(&["", "x", "", ""]), owned(&["", "x"]));
    }

    #[test]
    fn join_uses_one_space_and_drops_leading_whitespace() {
        assert_eq!(join_lines(&["  first", "\t second", "   third  "]), "  first second third  ");
    }

    #[test]
    fn join_of_one_line_is_that_line() {
        assert_eq!(join_lines(&["only"]), "only");
        assert_eq!(join_lines(&[]), "");
    }

    #[test]
    fn trim_removes_trailing_spaces_and_tabs_only() {
        assert_eq!(
            trim_trailing_whitespace(&["a  ", "\tb\t", "c", "   ", "é \u{3000}"]),
            owned(&["a", "\tb", "c", "", "é"])
        );
    }

    #[test]
    fn apply_dispatches_every_op() {
        let lines = ["b ", "a", "b "];
        assert_eq!(apply(LineOp::SortAscending, &lines, ci), Ok(owned(&["a", "b ", "b "])));
        assert_eq!(apply(LineOp::SortDescending, &lines, ci), Ok(owned(&["b ", "b ", "a"])));
        assert_eq!(apply(LineOp::RemoveDuplicates, &lines, ci), Ok(owned(&["b ", "a"])));
        assert_eq!(apply(LineOp::Join, &lines, ci), Ok(owned(&["b  a b "])));
        assert_eq!(apply(LineOp::TrimTrailing, &lines, ci), Ok(owned(&["b", "a", "b"])));
    }

    #[test]
    fn non_sort_ops_never_call_the_comparison() {
        let failing = |_: &str, _: &str| -> Result<Ordering, String> { Err("called".into()) };
        for op in [LineOp::RemoveDuplicates, LineOp::Join, LineOp::TrimTrailing] {
            assert!(apply(op, &["b", "a"], failing).is_ok(), "{op:?}");
        }
    }
}
