//! CalcTable on calc pages (`build-charts-2`).
//!
//! **Responsibility.** The outputs of the case named by the page's `Fixture:` line,
//! figures from the calc core only, never typed or copied.

use askama::Template;
use fieldbook_calc::Value;
use fieldbook_core::ids::CaseId;

struct CalcTableRow {
    name: String,
    unit: String,
    value: String,
    negative: bool,
}

#[derive(Template)]
#[template(path = "parts/calc_table.html")]
struct CalcTableTemplate {
    caption: String,
    head: [&'static str; 3],
    rows: Vec<CalcTableRow>,
    rounding: &'static str,
    result_class: &'static str,
    result_text: String,
}

/// Extracts the case identifier from the first `Fixture: <id>` line in `body`.
#[must_use]
pub fn fixture_id(body: &str) -> Option<&str> {
    for raw_line in body.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if let Some(id) = line.strip_prefix("Fixture: ")
            && CaseId::parse(id).is_ok()
        {
            return Some(id);
        }
    }
    None
}

/// Renders the HTML of the CalcTable for a calc page whose Markdown body is `body`.
#[must_use]
pub fn calc_table(body: &str) -> String {
    let unavailable =
        || crate::source::render_unavailable("The calculation table", "the calculation case runs");
    let Some(id) = fixture_id(body) else {
        return unavailable();
    };
    let Some(Ok(outcome)) = fieldbook_core::cases::run(id) else {
        return unavailable();
    };

    let rows: Vec<CalcTableRow> = outcome
        .outputs
        .iter()
        .map(|out| {
            let unit = match &out.currency {
                Some(cur) => cur.as_str().to_owned(),
                None => out.unit.clone(),
            };
            let (value, negative) = match out.value {
                Value::Known(d) => (crate::charts::format_amount(d), d.is_negative()),
                Value::Unknown(_) => (String::from("Unknown"), false),
            };
            CalcTableRow {
                name: out.name.clone(),
                unit,
                value,
                negative,
            }
        })
        .collect();

    let (result_class, result_text) = match fieldbook_core::cases::verify(id) {
        Some(Ok(())) => ("ok", format!("Fixture {id}: pass")),
        Some(Err(mismatches)) => {
            let mut text = format!("Fixture {id}: fail");
            for m in &mismatches {
                text.push_str(&format!(
                    "; {}: expected {}, found {}",
                    m.output, m.expected, m.found
                ));
            }
            ("bad", text)
        }
        None => ("bad", format!("Fixture {id}: fail")),
    };

    let tpl = CalcTableTemplate {
        caption: format!("Calculation case {id}"),
        head: ["Output", "Unit", "Value"],
        rows,
        rounding: fieldbook_core::strings::rounding(),
        result_class,
        result_text,
    };
    tpl.render()
        .unwrap_or_else(|e| unreachable!("calc_table.html is infallible: {e}"))
}
