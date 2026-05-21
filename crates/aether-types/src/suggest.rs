//! Suggestion engine: Levenshtein-based "did you mean?" for diagnostics.

/// Compute the edit distance between two strings (Wagner-Fischer, O(m*n)).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    // dp[i][j] = edit distance between a[..i] and b[..j].
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1])
            };
        }
    }

    dp[m][n]
}

/// Return the best candidate if its edit distance from `needle` is ≤
/// `max_distance` **and** it is strictly closer than the runner-up.
///
/// Returns `None` when:
/// - `needle` already matches a candidate exactly (no suggestion needed), or
/// - no candidate is within `max_distance`, or
/// - the best and second-best are tied (ambiguous — don't guess).
pub fn closest_match(needle: &str, candidates: &[String], max_distance: usize) -> Option<String> {
    let mut best_dist = usize::MAX;
    let mut best_name: Option<&str> = None;
    let mut runner_up_dist = usize::MAX;

    for cand in candidates {
        let d = levenshtein(needle, cand.as_str());
        if d == 0 {
            // Exact match — no suggestion needed.
            return None;
        }
        if d < best_dist {
            runner_up_dist = best_dist;
            best_dist = d;
            best_name = Some(cand.as_str());
        } else if d < runner_up_dist {
            runner_up_dist = d;
        }
    }

    // Must be within budget and strictly closer than the next candidate.
    if best_dist <= max_distance && best_dist < runner_up_dist {
        best_name.map(str::to_owned)
    } else {
        None
    }
}

// ─── unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cands(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// Exact match returns None — the identifier was spelled correctly.
    #[test]
    fn exact_match_returns_none() {
        let result = closest_match("print", &cands(&["print", "println", "eprint"]), 2);
        assert_eq!(result, None, "exact match should not produce a suggestion");
    }

    /// A one-character typo ("prnt" vs "print") should suggest the close match.
    #[test]
    fn close_match_returns_candidate() {
        let result = closest_match("prnt", &cands(&["print", "println", "eprint"]), 2);
        assert_eq!(result, Some("print".to_string()));
    }

    /// Completely unrelated name — nothing within distance 2.
    #[test]
    fn no_match_for_random_text() {
        let result = closest_match("asdfqwerty", &cands(&["print", "len", "push"]), 2);
        assert_eq!(result, None, "no suggestion expected for distant input");
    }

    /// Two equally close candidates — return None to avoid guessing.
    #[test]
    fn tied_near_matches_return_none() {
        // "fo" is distance 1 from both "foo" and "for", so tied.
        let result = closest_match("fo", &cands(&["foo", "for"]), 2);
        assert_eq!(result, None, "tied candidates should not produce a suggestion");
    }
}
