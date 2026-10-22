//! A plain aligned table for people reading a terminal.

use crate::model::EndpointReport;

const HEADERS: [&str; 8] = [
    "CHAIN", "TYPE", "ENDPOINT", "STATUS", "HEIGHT", "BEHIND", "LATENCY", "ERROR",
];

/// Columns that hold numbers are aligned to the right.
const RIGHT_ALIGNED: [bool; 8] = [false, false, false, false, true, true, true, false];

/// Longer errors are cut in the table so rows stay readable.
/// `--json` always has the full text.
const MAX_ERROR_CHARS: usize = 80;

/// Render rows as a table. Rows are printed in the order given.
pub fn render(reports: &[EndpointReport]) -> String {
    let rows: Vec<[String; 8]> = reports.iter().map(cells).collect();

    let mut widths = HEADERS.map(|h| h.chars().count());
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row.iter()) {
            *width = (*width).max(cell.chars().count());
        }
    }

    let header = HEADERS.map(str::to_string);
    let mut out = String::new();
    for row in std::iter::once(&header).chain(rows.iter()) {
        out.push_str(&format_row(row, &widths));
        out.push('\n');
    }
    out
}

fn cells(report: &EndpointReport) -> [String; 8] {
    [
        report.chain.clone(),
        report.chain_type.as_str().to_string(),
        report.endpoint.clone(),
        report.status.as_str().to_string(),
        or_dash(report.height),
        or_dash(report.behind),
        format!("{} ms", report.latency_ms),
        shorten(report.error.as_deref().unwrap_or_default(), MAX_ERROR_CHARS),
    ]
}

/// Keep the first `max` characters of `text`, marking a cut with "...".
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(3)).collect();
    format!("{kept}...")
}

fn or_dash(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| v.to_string())
}

fn format_row(row: &[String; 8], widths: &[usize; 8]) -> String {
    let mut parts = Vec::with_capacity(row.len());
    for (i, cell) in row.iter().enumerate() {
        let width = widths[i];
        let part = if RIGHT_ALIGNED[i] {
            format!("{cell:>width$}")
        } else {
            format!("{cell:<width$}")
        };
        parts.push(part);
    }
    // The last column is left-aligned, so drop its padding at the end of the line.
    parts.join("  ").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChainType, Status};

    #[test]
    fn long_errors_are_cut_to_80_chars_in_the_table() {
        let long = "x".repeat(200);
        let cut = shorten(&long, MAX_ERROR_CHARS);
        assert_eq!(cut.chars().count(), 80);
        assert!(cut.ends_with("..."));
        assert_eq!(
            shorten("HTTP 502 Bad Gateway", MAX_ERROR_CHARS),
            "HTTP 502 Bad Gateway"
        );
    }

    #[test]
    fn columns_line_up() {
        let rows = vec![
            EndpointReport {
                chain: "atomone".into(),
                chain_type: ChainType::Cosmos,
                endpoint: "https://a".into(),
                status: Status::Ok,
                height: Some(10_924_691),
                behind: Some(0),
                latency_ms: 54,
                chain_id: Some("atomone-1".into()),
                error: None,
            },
            EndpointReport {
                chain: "gno".into(),
                chain_type: ChainType::Tm2,
                endpoint: "https://gno.example".into(),
                status: Status::Down,
                height: None,
                behind: None,
                latency_ms: 5000,
                chain_id: None,
                error: Some("timed out after 5000 ms".into()),
            },
        ];
        let expected = "\
CHAIN    TYPE    ENDPOINT             STATUS    HEIGHT  BEHIND  LATENCY  ERROR
atomone  cosmos  https://a            ok      10924691       0    54 ms
gno      tm2     https://gno.example  down           -       -  5000 ms  timed out after 5000 ms
";
        assert_eq!(render(&rows), expected);
    }
}
