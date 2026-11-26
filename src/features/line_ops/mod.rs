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
    let mut sorted: Vec<&str> = lines.to_vec();
    let mut failure: Option<E> = None;
    // sort_by is a stable merge sort. Once a comparison failed, the rest
    // answer Equal so the sort ends; the error is returned below.
    sorted.sort_by(|a, b| {
        if failure.is_some() {
            return Ordering::Equal;
        }
        let r = if descending { cmp(b, a) } else { cmp(a, b) };
        match r {
            Ok(o) => o,
            Err(e) => {
                failure = Some(e);
                Ordering::Equal
            }
        }
    });
    match failure {
        Some(e) => Err(e),
        None => Ok(sorted.into_iter().map(str::to_string).collect()),
    }
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
