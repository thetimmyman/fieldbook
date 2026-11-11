//! Charts on the area pages (`build-charts`).
//!
//! **Responsibility.** ChartPanel (title, headline figure in HTML, plot, ChartFoot, TableFold)
//! or an Unavailable panel; figures come from the calc core only; no text inside the SVG.

use std::cmp::Ordering;

use askama::Template;
use fieldbook_calc::{Dec, Outcome, Value, sub_dec};

/// Identifier of the calculation case used for the waterfall chart.
pub const WATERFALL_CASE: &str = "CALC-02";

const NO_SERIES: &str =
    "No Corpus file format stores a data series yet, so the build draws no line and no sample one.";
const BID_DATA: &str =
    "Past bids, prices and win rates are company data. They arrive with the company overlay.";
const RTE_DATA: &str =
    "Actual and provisional rates are company data. They arrive with the company overlay.";
const CASE_RUNS: &str = "the calculation case runs";
const NO_SAMPLE: &str =
    "The calc core did not produce the case, so the build draws no bars and no sample.";
const DATA_SERIES: &str = "the Corpus holds a data series";

/// One step or total bar in a waterfall chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    /// The name of the calculation output this bar ends at.
    pub output: &'static str,
    /// The row label.
    pub label: &'static str,
    /// Where the bar starts: zero, or the previous output.
    pub from: Dec,
    /// Where the bar ends: this output.
    pub to: Dec,
    /// `to` minus `from`, computed by the calc core.
    pub step: Dec,
    /// `true` for the last bar, which wears the total colour.
    pub total: bool,
}

fn sub(to: Dec, from: Dec) -> Option<Dec> {
    match sub_dec(Value::Known(to), Value::Known(from)).ok()? {
        Value::Known(s) => Some(s),
        _ => None,
    }
}

/// Computes the waterfall bars from an evaluated calculation outcome.
#[must_use]
pub fn waterfall_bars(outcome: &Outcome) -> Option<Vec<Bar>> {
    let mut bars = Vec::with_capacity(3);
    let (mut prev, mut max) = (Dec::ZERO, Dec::ZERO);
    let steps = [
        ("landed", "Landed cost"),
        ("cost", "Cost increment"),
        ("price", "Price"),
    ];
    for (output, label) in steps {
        let out = outcome.outputs.iter().find(|o| o.name == output)?;
        let to = match out.value {
            Value::Known(d) if !d.is_negative() => d,
            _ => return None,
        };
        let step = sub(to, prev)?;
        max = max.max(to);
        let total = bars.len() == 2;
        bars.push(Bar {
            output,
            label,
            from: prev,
            to,
            step,
            total,
        });
        prev = to;
    }
    (!max.is_zero()).then_some(bars)
}

fn hundredths(v: Dec, max: Dec) -> usize {
    let (vs, ms) = (v.scale(), max.scale());
    let sc = |m: i128, d| 10_i128.checked_pow(d).and_then(|p| m.checked_mul(p));
    let (vm, mm) = match vs.cmp(&ms) {
        Ordering::Less => (sc(v.mantissa(), ms - vs).unwrap_or(0), max.mantissa()),
        Ordering::Greater => (v.mantissa(), sc(max.mantissa(), vs - ms).unwrap_or(0)),
        Ordering::Equal => (v.mantissa(), max.mantissa()),
    };
    if mm <= 0 || vm <= 0 {
        return 0;
    }
    let dividend = vm.checked_mul(20_000).and_then(|x| x.checked_add(mm));
    let denom = mm.checked_mul(2);
    let n = dividend.zip(denom).map(|(n, d)| n / d).unwrap_or(0);
    usize::try_from(n.clamp(0, 10_000)).unwrap_or(0)
}

/// Formats a decimal with thousands separators in the integer part, wrapping negative amounts
/// in parentheses with no minus sign.
#[must_use]
pub fn format_amount(d: Dec) -> String {
    let s = d.to_string();
    let is_neg = s.starts_with('-');
    let rest = s.strip_prefix('-').unwrap_or(&s);
    let (ip, fp) = rest.split_once('.').unwrap_or((rest, ""));
    let mut out = String::from(if is_neg { "(" } else { "" });
    for (i, ch) in ip.chars().enumerate() {
        out.push(ch);
        let rem = ip.len().saturating_sub(i + 1);
        if rem > 0 && rem % 3 == 0 {
            out.push(',');
        }
    }
    if !fp.is_empty() {
        out.push('.');
        out.push_str(fp);
    }
    if is_neg {
        out.push(')');
    }
    out
}

fn valid_date(s: &str) -> bool {
    s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
}

fn case_as_of(id: &str) -> Option<String> {
    let target = format!("id = \"{id}\"");
    let (_, raw) = fieldbook_core::cases::CASES
        .iter()
        .find(|(_, r)| r.lines().any(|l| l.trim() == target))?;
    raw.lines().find_map(|l| {
        let (v, _) = l.trim().strip_prefix("as_of = \"")?.split_once('"')?;
        valid_date(v).then(|| v.to_owned())
    })
}

struct Figure {
    value: String,
    meaning: String,
}
struct TableFold {
    caption: String,
    head: Vec<&'static str>,
    rows: Vec<Vec<String>>,
}
struct Unavailable {
    html: String,
    why: String,
}

#[derive(Default, Template)]
#[template(path = "parts/chart_panel.html")]
struct ChartPanel {
    title: String,
    figure: Option<Figure>,
    plot_html: String,
    source: String,
    as_of: Option<String>,
    origin: String,
    table: Option<TableFold>,
    unavailable: Option<Unavailable>,
}

/// Renders an unavailable chart panel with a reason.
#[must_use]
pub fn unavailable_panel(title: &str, thing: &str, until: &str, why: &str) -> String {
    let unavailable = Some(Unavailable {
        html: crate::source::render_unavailable(thing, until),
        why: why.to_owned(),
    });
    ChartPanel {
        title: title.to_owned(),
        unavailable,
        ..Default::default()
    }
    .render()
    .unwrap_or_else(|e| unreachable!("chart_panel.html is infallible: {e}"))
}

fn bar_row(i: usize, b: &Bar, max: Dec) -> String {
    let (s, t) = (format_amount(b.step), format_amount(b.to));
    let tip = format!("{}: adds {s}, running {t}", b.label);
    let (f, to) = (hundredths(b.from, max), hundredths(b.to, max));
    let track = crate::svg::bar_track(f, to, b.total, &tip);
    let (lbl, val) = match (i, b.total) {
        (_, true) => (
            format!("<b>{}</b><small>adds {s} to cost</small>", b.label),
            format!("<b>{t}</b>"),
        ),
        (0, _) => (b.label.to_owned(), s),
        _ => (
            format!("{}<small>running cost {t}</small>", b.label),
            format!("+{s}"),
        ),
    };
    let row = format!("<div class=\"hb-l\">{lbl}</div>{track}<div class=\"hb-v\">{val}</div>");
    format!("<div class=\"hb-row\">{row}</div>")
}

fn bar_cells(b: &Bar) -> Vec<String> {
    let (s, t) = (format_amount(b.step), format_amount(b.to));
    vec![b.label.to_owned(), s, t]
}

fn waterfall_table(bars: &[Bar]) -> TableFold {
    TableFold {
        caption: format!("Price build-up, {WATERFALL_CASE}"),
        head: vec!["Step", "Adds", "Running"],
        rows: bars.iter().map(bar_cells).collect(),
    }
}

/// Renders the price build-up waterfall chart panel.
#[must_use]
pub fn rate_build_up_panel() -> String {
    let unavail =
        || unavailable_panel("Price build-up", "The price build-up", CASE_RUNS, NO_SAMPLE);
    let Some((outcome, bars)) = fieldbook_core::cases::run(WATERFALL_CASE)
        .and_then(Result::ok)
        .and_then(|o| waterfall_bars(&o).map(|b| (o, b)))
    else {
        return unavail();
    };
    let Some(last) = bars.last() else {
        return unavail();
    };

    let find = |name| outcome.outputs.iter().find(|o| o.name == name);
    let cur = find("price")
        .and_then(|o| o.currency.as_ref())
        .map_or("", |c| c.as_str());
    let sp = if cur.is_empty() { "" } else { " " };
    let p_sent = format!("{cur}{sp}price of the case lot, scale from 0.");
    let u_sent = match find("unit").map(|o| &o.value) {
        Some(Value::Known(u)) => format!(
            " Unit price {} per unit is shown separately and is not a step.",
            format_amount(*u)
        ),
        _ => String::new(),
    };

    let max = bars.iter().map(|b| b.to).max().unwrap_or(Dec::ZERO);
    let rows_html: String = bars
        .iter()
        .enumerate()
        .map(|(i, b)| bar_row(i, b, max))
        .collect();
    let lbl = fieldbook_core::strings::origin_labels()
        .get(1)
        .copied()
        .unwrap_or("");
    let origin = format!("{lbl} · calc core, not a model");

    ChartPanel {
        title: "Price build-up".to_owned(),
        figure: Some(Figure {
            value: format_amount(last.to),
            meaning: format!("{p_sent}{u_sent}"),
        }),
        plot_html: format!("<div class=\"hbars\">{rows_html}</div>"),
        source: format!("Source: {WATERFALL_CASE}, a synthetic calculation case"),
        as_of: case_as_of(WATERFALL_CASE),
        origin,
        table: Some(waterfall_table(&bars)),
        ..Default::default()
    }
    .render()
    .unwrap_or_else(|e| unreachable!("chart_panel.html is infallible: {e}"))
}

/// Renders the charts section for an area page by area code.
#[must_use]
pub fn area_charts(area_code: &str) -> String {
    let un = unavailable_panel;
    let r2 = |t, w| un(t, t, "Release 2", w);
    if area_code.eq_ignore_ascii_case("est") {
        let p1 = un(
            "Escalation index",
            "The escalation index",
            DATA_SERIES,
            NO_SERIES,
        );
        let p2 = r2("Bid history", BID_DATA);
        format!("<div class=\"chart-row\">{p1}{p2}</div>")
    } else if area_code.eq_ignore_ascii_case("rte") {
        let p1 = rate_build_up_panel();
        let p2 = r2("Rate history", RTE_DATA);
        format!("<div class=\"chart-row\">{p1}{p2}</div>")
    } else {
        String::new()
    }
}
