// Line diff for Tab > Compare with Last Save: a unified diff between the
// text of the last save and the text being edited. Pure text work, nothing
// stored: both texts come from memory every time the dialog opens.
//
// Myers' O((N+M)D) algorithm in linear space (middle snake, divide and
// conquer), on lines mapped to integers. The editor gives "\r\n" (or "\r")
// where the payload may hold "\n": every one of them ends a line, so line
// endings alone are never a change.

use std::collections::HashMap;

/// Above this many lines on either side, no diff is computed (the dialog
/// says so): the worst case of Myers is quadratic in time.
pub const MAX_LINES: usize = 20_000;

/// Unchanged lines shown around each change.
pub const CONTEXT: usize = 3;

/// What Compare with Last Save shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparison {
    /// Same lines on both sides.
    Identical,
    /// One side has more than `MAX_LINES` lines: not compared.
    TooLarge { saved_lines: usize, current_lines: usize },
    /// The unified diff, one entry per line: "@@ -a,b +c,d @@" headers,
    /// then "- ", "+ " and "  " lines.
    Diff(Vec<String>),
}

/// A diff that could not be computed (a bug of the algorithm, never the
/// input): the caller says so instead of showing a wrong diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    /// No middle snake within the bound the theorem gives.
    NoMiddleSnake { n: usize, m: usize },
    /// The split point would not make the problem smaller.
    NoProgress { n: usize, m: usize, x: usize, y: usize },
}

/// One step of the edit script, from the saved text to the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal,
    Delete,
    Insert,
}

/// The lines of `text`: "\r\n", "\r" and "\n" each end a line; a final line
/// break does not start an empty line ("a\n" is one line, "" none).
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            // '\r' and '\n' are ASCII: `i` is a char boundary.
            b'\r' => {
                lines.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Compare the saved text with the current one (see `Comparison`).
pub fn compare(saved: &str, current: &str) -> Result<Comparison, DiffError> {
    let a = split_lines(saved);
    let b = split_lines(current);
    if a.len() > MAX_LINES || b.len() > MAX_LINES {
        return Ok(Comparison::TooLarge { saved_lines: a.len(), current_lines: b.len() });
    }
    if a == b {
        return Ok(Comparison::Identical);
    }
    // Lines become integers once, so the search compares numbers.
    let mut ids: HashMap<&str, u32> = HashMap::new();
    let ai = intern(&a, &mut ids);
    let bi = intern(&b, &mut ids);
    let ops = edit_script(&ai, &bi)?;
    Ok(Comparison::Diff(unified(&a, &b, &ops, CONTEXT)))
}

/// The number of each line; equal lines get the same number. At most
/// 2 * MAX_LINES distinct lines: they fit in a u32.
fn intern<'a>(lines: &[&'a str], ids: &mut HashMap<&'a str, u32>) -> Vec<u32> {
    lines
        .iter()
        .map(|&line| {
            let next = ids.len() as u32;
            *ids.entry(line).or_insert(next)
        })
        .collect()
}

/// Diagonal-indexed furthest x of Myers, for k in -max..=max.
struct V {
    offset: isize,
    v: Vec<isize>,
}

impl V {
    fn new(max: usize) -> V {
        V { offset: max as isize, v: vec![0; 2 * max + 1] }
    }
    fn get(&self, k: isize) -> isize {
        self.v[(k + self.offset) as usize]
    }
    fn set(&mut self, k: isize, x: isize) {
        self.v[(k + self.offset) as usize] = x;
    }
}

/// The shortest edit script from `a` to `b`; within a run of changes the
/// deletions come before the insertions (as `diff -u` prints them).
fn edit_script(a: &[u32], b: &[u32]) -> Result<Vec<Op>, DiffError> {
    // d never exceeds ceil((n+m)/2); the reads go one diagonal beyond.
    let max = (a.len() + b.len()).div_ceil(2) + 2;
    let mut vf = V::new(max);
    let mut vb = V::new(max);
    let mut ops = Vec::with_capacity(a.len() + b.len());
    conquer(a, b, &mut vf, &mut vb, &mut ops)?;
    // Group each run of changes: deletions first.
    let mut i = 0;
    while i < ops.len() {
        if ops[i] == Op::Equal {
            i += 1;
            continue;
        }
        let end = ops[i..].iter().position(|&o| o == Op::Equal).map_or(ops.len(), |p| i + p);
        ops[i..end].sort_by_key(|&o| o != Op::Delete);
        i = end;
    }
    Ok(ops)
}

fn conquer(a: &[u32], b: &[u32], vf: &mut V, vb: &mut V, ops: &mut Vec<Op>) -> Result<(), DiffError> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    ops.extend(std::iter::repeat_n(Op::Equal, prefix));
    let (a, b) = (&a[prefix..], &b[prefix..]);
    let suffix = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[..a.len() - suffix], &b[..b.len() - suffix]);
    if a.is_empty() {
        ops.extend(std::iter::repeat_n(Op::Insert, b.len()));
    } else if b.is_empty() {
        ops.extend(std::iter::repeat_n(Op::Delete, a.len()));
    } else {
        let (x, y) = middle_snake(a, b, vf, vb)?;
        // Both sides non-empty and trimmed: the distance is at least 2 and
        // the split lies strictly inside; checked, never assumed.
        if (x == 0 && y == 0) || (x == a.len() && y == b.len()) || x > a.len() || y > b.len() {
            return Err(DiffError::NoProgress { n: a.len(), m: b.len(), x, y });
        }
        conquer(&a[..x], &b[..y], vf, vb, ops)?;
        conquer(&a[x..], &b[y..], vf, vb, ops)?;
    }
    ops.extend(std::iter::repeat_n(Op::Equal, suffix));
    Ok(())
}

/// A point of an optimal path halfway through (Myers 1986, section 4b),
/// where the forward search from the start meets the backward one from
/// the end. `vb` holds x counted from the end, on the reversed diagonals.
fn middle_snake(a: &[u32], b: &[u32], vf: &mut V, vb: &mut V) -> Result<(usize, usize), DiffError> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let delta = n - m;
    let odd = delta & 1 != 0;
    vf.set(1, 0);
    vb.set(1, 0);
    let d_max = (n + m + 1) / 2;
    for d in 0..=d_max {
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && vf.get(k - 1) < vf.get(k + 1)) { vf.get(k + 1) } else { vf.get(k - 1) + 1 };
            let mut y = x - k;
            let (x0, y0) = (x, y);
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            vf.set(k, x);
            if odd && (k - delta).abs() < d && x + vb.get(delta - k) >= n {
                return Ok((x0 as usize, y0 as usize));
            }
            k += 2;
        }
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && vb.get(k - 1) < vb.get(k + 1)) { vb.get(k + 1) } else { vb.get(k - 1) + 1 };
            let mut y = x - k;
            while x < n && y < m && a[(n - x - 1) as usize] == b[(m - y - 1) as usize] {
                x += 1;
                y += 1;
            }
            vb.set(k, x);
            if !odd && (k - delta).abs() <= d && x + vf.get(delta - k) >= n {
                return Ok(((n - x) as usize, (m - y) as usize));
            }
            k += 2;
        }
    }
    Err(DiffError::NoMiddleSnake { n: a.len(), m: b.len() })
}

/// The unified diff of `ops` (an edit script from `a` to `b`) with
/// `context` unchanged lines around each change; hunks closer than twice
/// the context merge. A side with no line in a hunk names the line before
/// it ("-0,0" at the top), as `diff -u` does.
fn unified(a: &[&str], b: &[&str], ops: &[Op], context: usize) -> Vec<String> {
    // Position in a and b before each op.
    let mut pos = Vec::with_capacity(ops.len() + 1);
    let (mut i, mut j) = (0usize, 0usize);
    for op in ops {
        pos.push((i, j));
        match op {
            Op::Equal => {
                i += 1;
                j += 1;
            }
            Op::Delete => i += 1,
            Op::Insert => j += 1,
        }
    }
    pos.push((i, j));

    let changes: Vec<usize> = ops.iter().enumerate().filter(|(_, &o)| o != Op::Equal).map(|(k, _)| k).collect();
    let mut out = Vec::new();
    let mut c = 0;
    while c < changes.len() {
        let first = changes[c];
        let mut last = first;
        while c + 1 < changes.len() && changes[c + 1] - last - 1 <= 2 * context {
            c += 1;
            last = changes[c];
        }
        c += 1;
        let start = first.saturating_sub(context);
        let end = (last + 1 + context).min(ops.len());
        let (a0, b0) = pos[start];
        let (a1, b1) = pos[end];
        let (a_len, b_len) = (a1 - a0, b1 - b0);
        let a_line = if a_len == 0 { a0 } else { a0 + 1 };
        let b_line = if b_len == 0 { b0 } else { b0 + 1 };
        out.push(format!("@@ -{},{} +{},{} @@", a_line, a_len, b_line, b_len));
        for k in start..end {
            let (i, j) = pos[k];
            out.push(match ops[k] {
                Op::Equal => format!("  {}", a[i]),
                Op::Delete => format!("- {}", a[i]),
                Op::Insert => format!("+ {}", b[j]),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(saved: &str, current: &str) -> Vec<String> {
        match compare(saved, current).unwrap() {
            Comparison::Diff(lines) => lines,
            other => panic!("expected a diff, got {:?}", other),
        }
    }

    #[test]
    fn lines_end_on_crlf_cr_or_lf() {
        assert_eq!(split_lines(""), Vec::<&str>::new());
        assert_eq!(split_lines("a"), ["a"]);
        assert_eq!(split_lines("a\n"), ["a"]);
        assert_eq!(split_lines("\n"), [""]);
        assert_eq!(split_lines("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\r\n\r\nb"), ["a", "", "b"]);
        assert_eq!(split_lines("\u{e9}t\u{e9}\r\u{1f600}"), ["\u{e9}t\u{e9}", "\u{1f600}"]);
    }

    #[test]
    fn same_text_is_identical() {
        assert_eq!(compare("", ""), Ok(Comparison::Identical));
        assert_eq!(compare("a\nb", "a\nb"), Ok(Comparison::Identical));
    }

    /// The editor's "\r\n" against the payload's "\n" is not a change.
    #[test]
    fn line_endings_alone_are_no_change() {
        assert_eq!(compare("a\nb\n", "a\r\nb"), Ok(Comparison::Identical));
        assert_eq!(compare("a\rb", "a\nb"), Ok(Comparison::Identical));
    }

    #[test]
    fn one_changed_line_gives_one_hunk_with_context() {
        let saved = "1\n2\n3\n4\n5\n6\n7\n8\n9";
        let current = "1\n2\n3\n4\nfive\n6\n7\n8\n9";
        assert_eq!(
            diff(saved, current),
            ["@@ -2,7 +2,7 @@", "  2", "  3", "  4", "- 5", "+ five", "  6", "  7", "  8"]
        );
    }

    #[test]
    fn context_is_cut_at_the_edges() {
        assert_eq!(diff("a\nb", "x\nb"), ["@@ -1,2 +1,2 @@", "- a", "+ x", "  b"]);
        assert_eq!(diff("a\nb", "a\nx"), ["@@ -1,2 +1,2 @@", "  a", "- b", "+ x"]);
    }

    #[test]
    fn empty_sides_name_the_line_before() {
        assert_eq!(diff("", "a\nb"), ["@@ -0,0 +1,2 @@", "+ a", "+ b"]);
        assert_eq!(diff("a\nb", ""), ["@@ -1,2 +0,0 @@", "- a", "- b"]);
        // A pure insertion after line 2 of 2.
        assert_eq!(diff("a\nb", "a\nb\nc"), ["@@ -1,2 +1,3 @@", "  a", "  b", "+ c"]);
        // A pure insertion with nothing kept before it in the hunk.
        assert_eq!(diff("1\n2\n3\n4\n5", "1\n2\n3\n4\nx\n5"), ["@@ -2,4 +2,5 @@", "  2", "  3", "  4", "+ x", "  5"]);
    }

    #[test]
    fn distant_changes_make_two_hunks_close_ones_merge() {
        let saved: Vec<String> = (1..=20).map(|i| i.to_string()).collect();
        let mut far = saved.clone();
        far[1] = "B".into(); // line 2
        far[17] = "R".into(); // line 18
        let lines = diff(&saved.join("\n"), &far.join("\n"));
        let headers: Vec<&String> = lines.iter().filter(|l| l.starts_with("@@")).collect();
        assert_eq!(headers, ["@@ -1,5 +1,5 @@", "@@ -15,6 +15,6 @@"]);

        // 6 unchanged lines between two changes (= 2 * CONTEXT): one hunk.
        let mut near = saved.clone();
        near[1] = "B".into(); // line 2
        near[8] = "I".into(); // line 9
        let lines = diff(&saved.join("\n"), &near.join("\n"));
        let headers: Vec<&String> = lines.iter().filter(|l| l.starts_with("@@")).collect();
        assert_eq!(headers, ["@@ -1,12 +1,12 @@"]);
        // 7 unchanged lines: two hunks.
        let mut apart = saved.clone();
        apart[1] = "B".into(); // line 2
        apart[9] = "J".into(); // line 10
        let lines = diff(&saved.join("\n"), &apart.join("\n"));
        assert_eq!(lines.iter().filter(|l| l.starts_with("@@")).count(), 2);
    }

    #[test]
    fn deletions_come_before_insertions_in_a_run() {
        assert_eq!(diff("a\nb\nc", "x\ny\nz"), ["@@ -1,3 +1,3 @@", "- a", "- b", "- c", "+ x", "+ y", "+ z"]);
    }

    #[test]
    fn non_ascii_lines_are_kept_whole() {
        assert_eq!(diff("caf\u{e9}\n\u{1f600}", "caf\u{e9}\n\u{1f601}"), ["@@ -1,2 +1,2 @@", "  caf\u{e9}", "- \u{1f600}", "+ \u{1f601}"]);
    }

    #[test]
    fn more_than_the_limit_is_not_compared() {
        let big = "x\n".repeat(MAX_LINES + 1);
        let ok = "x\n".repeat(MAX_LINES);
        assert_eq!(
            compare(&big, "a"),
            Ok(Comparison::TooLarge { saved_lines: MAX_LINES + 1, current_lines: 1 })
        );
        assert_eq!(
            compare("a", &big),
            Ok(Comparison::TooLarge { saved_lines: 1, current_lines: MAX_LINES + 1 })
        );
        // Exactly the limit is compared.
        let edited = format!("y\n{}", "x\n".repeat(MAX_LINES - 1));
        assert!(matches!(compare(&ok, &edited), Ok(Comparison::Diff(_))));
    }

    /// Length of the longest common subsequence, by dynamic programming:
    /// the reference the shortest edit script must meet.
    fn lcs_len(a: &[u32], b: &[u32]) -> usize {
        let mut row = vec![0usize; b.len() + 1];
        for x in a {
            let mut diag = 0;
            for (j, y) in b.iter().enumerate() {
                let up = row[j + 1];
                row[j + 1] = if x == y { diag + 1 } else { up.max(row[j]) };
                diag = up;
            }
        }
        row[b.len()]
    }

    /// Replays the script on `a`; it must give `b`.
    fn apply(a: &[u32], b: &[u32], ops: &[Op]) -> Vec<u32> {
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::new();
        for op in ops {
            match op {
                Op::Equal => {
                    assert_eq!(a[i], b[j], "Equal on different lines");
                    out.push(a[i]);
                    i += 1;
                    j += 1;
                }
                Op::Delete => i += 1,
                Op::Insert => {
                    out.push(b[j]);
                    j += 1;
                }
            }
        }
        assert_eq!((i, j), (a.len(), b.len()), "script does not cover both sides");
        out
    }

    /// Random sequences over small alphabets: the script is valid and
    /// minimal (as many edits as n + m - 2 * LCS).
    #[test]
    fn script_is_valid_and_minimal_on_random_input() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for round in 0..3000 {
            let alphabet = 1 + next() % 5;
            let a: Vec<u32> = (0..next() % 25).map(|_| (next() % alphabet) as u32).collect();
            let b: Vec<u32> = (0..next() % 25).map(|_| (next() % alphabet) as u32).collect();
            let ops = edit_script(&a, &b).unwrap_or_else(|e| panic!("round {round}: {e:?} a={a:?} b={b:?}"));
            assert_eq!(apply(&a, &b, &ops), b, "round {round}: a={a:?} b={b:?}");
            let edits = ops.iter().filter(|&&o| o != Op::Equal).count();
            assert_eq!(edits, a.len() + b.len() - 2 * lcs_len(&a, &b), "round {round}: not minimal a={a:?} b={b:?}");
        }
    }

    /// Two unrelated texts of 2000 lines (the quadratic case) finish.
    #[test]
    fn unrelated_texts_are_diffed() {
        let saved: String = (0..2000).map(|i| format!("old {}\n", i)).collect();
        let current: String = (0..2000).map(|i| format!("new {}\n", i)).collect();
        let lines = diff(&saved, &current);
        assert_eq!(lines[0], "@@ -1,2000 +1,2000 @@");
        assert_eq!(lines.len(), 4001);
    }
}
