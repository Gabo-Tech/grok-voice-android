//! Small on-device PDF writer for Finance summaries.
//! Standard fonts only. Charts are vector paths, not a screenshot.
//! The page is warm stone paper with charcoal ink and champagne rules,
//! so it stays readable when printed.

pub struct Page {
    pub title: String,
    pub subtitle: String,
    pub notes: Vec<String>,
    pub numbers: Vec<(String, String)>,
    pub rows: Vec<Row>,
    pub chart: Option<Chart>,
}

pub struct Row {
    pub name: String,
    pub detail: String,
    pub amount: String,
    pub color: String,
}

pub struct Slice {
    pub label: String,
    pub value: f64,
    pub color: String,
    pub caption: String,
}

pub enum Chart {
    Donut { center_label: String, center_value: String, slices: Vec<Slice> },
    Bars { rows: Vec<Slice> },
    Groups { captions: Vec<String>, series: Vec<String>, values: Vec<Vec<f64>>, colors: Vec<String> },
    Trend { labels: Vec<String>, income: Vec<f64>, expense: Vec<f64>, net: Vec<f64> },
}

const W: f64 = 595.28;
const H: f64 = 841.89;

pub fn render(pages: &[Page]) -> Vec<u8> {
    let pages = if pages.is_empty() {
        vec![Page { title: "Finance".into(), subtitle: String::new(), notes: vec!["Nothing to export yet".into()], numbers: vec![], rows: vec![], chart: None }]
    } else {
        pages.to_vec()
    };
    let mut streams = Vec::new();
    for page in &pages {
        streams.push(paint(page));
    }
    assemble(&streams)
}

impl Clone for Page {
    fn clone(&self) -> Self {
        Self { title: self.title.clone(), subtitle: self.subtitle.clone(), notes: self.notes.clone(), numbers: self.numbers.clone(), rows: self.rows.clone(), chart: self.chart.clone() }
    }
}

impl Clone for Row {
    fn clone(&self) -> Self {
        Self { name: self.name.clone(), detail: self.detail.clone(), amount: self.amount.clone(), color: self.color.clone() }
    }
}

impl Clone for Slice {
    fn clone(&self) -> Self {
        Self { label: self.label.clone(), value: self.value, color: self.color.clone(), caption: self.caption.clone() }
    }
}

impl Clone for Chart {
    fn clone(&self) -> Self {
        match self {
            Chart::Donut { center_label, center_value, slices } => Chart::Donut { center_label: center_label.clone(), center_value: center_value.clone(), slices: slices.clone() },
            Chart::Bars { rows } => Chart::Bars { rows: rows.clone() },
            Chart::Groups { captions, series, values, colors } => Chart::Groups { captions: captions.clone(), series: series.clone(), values: values.clone(), colors: colors.clone() },
            Chart::Trend { labels, income, expense, net } => Chart::Trend { labels: labels.clone(), income: income.clone(), expense: expense.clone(), net: net.clone() },
        }
    }
}

fn paint(page: &Page) -> String {
    let mut ops = String::new();
    rect(&mut ops, 0.0, 0.0, W, H, rgb("F4F2EE"));
    rect(&mut ops, 36.0, 36.0, W - 72.0, H - 72.0, rgb("FAF8F5"));
    stroke_rect(&mut ops, 36.0, 36.0, W - 72.0, H - 72.0, rgb("C4B5A0"), 0.8);
    text(&mut ops, 52.0, H - 78.0, 22.0, &page.title, true, rgb("1A1B1E"));
    if !page.subtitle.is_empty() {
        text(&mut ops, 52.0, H - 100.0, 11.0, &page.subtitle, false, rgb("6E6A64"));
    }
    let mut y = H - 128.0;
    for note in &page.notes {
        text(&mut ops, 52.0, y, 10.0, note, false, rgb("6E6A64"));
        y -= 16.0;
    }
    for (label, value) in &page.numbers {
        y -= 8.0;
        text(&mut ops, 52.0, y, 9.0, &label.to_uppercase(), false, rgb("B8A48A"));
        y -= 28.0;
        text(&mut ops, 52.0, y, 22.0, value, true, rgb("1A1B1E"));
        y -= 18.0;
    }
    if !page.rows.is_empty() {
        y -= 8.0;
        for row in &page.rows {
            if y < 72.0 {
                break;
            }
            swatch(&mut ops, 52.0, y - 2.0, &row.color);
            text(&mut ops, 70.0, y, 12.0, &clip(&row.name, 28), true, rgb("1A1B1E"));
            text(&mut ops, 250.0, y, 10.0, &clip(&row.detail, 22), false, rgb("6E6A64"));
            text(&mut ops, 400.0, y, 12.0, &row.amount, true, rgb("1A1B1E"));
            y -= 22.0;
        }
    }
    if let Some(chart) = &page.chart {
        match chart {
            Chart::Donut { center_label, center_value, slices } => donut(&mut ops, slices, center_label, center_value),
            Chart::Bars { rows } => bars(&mut ops, rows),
            Chart::Groups { captions, series, values, colors } => groups(&mut ops, captions, series, values, colors),
            Chart::Trend { labels, income, expense, net } => trend(&mut ops, labels, income, expense, net),
        }
    }
    ops
}

fn donut(ops: &mut String, slices: &[Slice], center_label: &str, center_value: &str) {
    let total: f64 = slices.iter().map(|slice| slice.value.abs()).sum();
    let cx = W / 2.0;
    let cy = 430.0;
    let outer = 118.0;
    let inner = 72.0;
    if total <= 0.0 {
        text(ops, 52.0, cy, 12.0, "Nothing to chart", false, rgb("6E6A64"));
        return;
    }
    let mut angle = -90.0_f64;
    for slice in slices {
        let sweep = 360.0 * (slice.value.abs() / total);
        wedge(ops, cx, cy, outer, inner, angle, angle + sweep, &slice.color);
        angle += sweep;
    }
    disk(ops, cx, cy, inner - 1.0, rgb("FAF8F5"));
    text(ops, cx - 70.0, cy + 8.0, 9.0, center_label, false, rgb("B8A48A"));
    text(ops, cx - 90.0, cy - 16.0, 14.0, center_value, true, rgb("1A1B1E"));
    let mut y = 250.0;
    for slice in slices.iter().take(8) {
        swatch(ops, 70.0, y - 2.0, &slice.color);
        text(ops, 88.0, y, 12.0, &clip(&slice.label, 24), false, rgb("1A1B1E"));
        text(ops, 360.0, y, 12.0, &slice.caption, true, rgb("1A1B1E"));
        y -= 22.0;
    }
}

fn bars(ops: &mut String, rows: &[Slice]) {
    let max = rows.iter().map(|row| row.value.abs()).fold(0.0_f64, f64::max).max(1.0);
    let mut y = 640.0;
    for row in rows.iter().take(12) {
        text(ops, 52.0, y + 18.0, 11.0, &clip(&row.label, 32), false, rgb("1A1B1E"));
        let width = ((row.value.abs() / max) * 360.0).max(2.0);
        rect(ops, 52.0, y, width, 14.0, parse_color(&row.color));
        text(ops, 420.0, y, 11.0, &row.caption, true, rgb("1A1B1E"));
        y -= 42.0;
        if y < 80.0 {
            break;
        }
    }
}

fn groups(ops: &mut String, captions: &[String], series: &[String], values: &[Vec<f64>], colors: &[String]) {
    let max = values.iter().flatten().map(|value| value.abs()).fold(0.0_f64, f64::max).max(1.0);
    let mut x = 70.0;
    let base = 180.0;
    let gap = ((W - 120.0) / (captions.len().max(1) as f64)).min(140.0);
    for (index, caption) in captions.iter().enumerate() {
        let mut bx = x;
        for (series_index, column) in values.iter().enumerate() {
            let value = column.get(index).copied().unwrap_or(0.0).abs();
            let height = (value / max) * 320.0;
            let color = colors.get(series_index).cloned().unwrap_or_else(|| "#6E8B74".into());
            rect(ops, bx, base, 18.0, height.max(1.0), parse_color(&color));
            bx += 22.0;
        }
        text(ops, x, base - 18.0, 10.0, caption, false, rgb("1A1B1E"));
        x += gap;
    }
    let mut y = 150.0;
    for (index, label) in series.iter().enumerate() {
        let color = colors.get(index).cloned().unwrap_or_else(|| "#1A1B1E".into());
        swatch(ops, 52.0, y - 2.0, &color);
        text(ops, 70.0, y, 11.0, label, false, rgb("1A1B1E"));
        y -= 18.0;
    }
}

fn trend(ops: &mut String, labels: &[String], income: &[f64], expense: &[f64], net: &[f64]) {
    let left = 64.0;
    let right = W - 52.0;
    let bottom = 220.0;
    let top = 620.0;
    stroke_rect(ops, left, bottom, right - left, top - bottom, rgb("E4DDD2"), 0.6);
    let max = income.iter().chain(expense).chain(net).map(|value| value.abs()).fold(1.0_f64, f64::max);
    line_series(ops, income, left, right, bottom, top, max, rgb("6E8B74"));
    line_series(ops, expense, left, right, bottom, top, max, rgb("A56B66"));
    line_series(ops, net, left, right, bottom, top, max, rgb("B8A48A"));
    let step = if labels.is_empty() { 0.0 } else { (right - left) / (labels.len().saturating_sub(1).max(1) as f64) };
    for (index, label) in labels.iter().enumerate() {
        text(ops, left + step * index as f64 - 8.0, bottom - 18.0, 9.0, label, false, rgb("6E6A64"));
    }
    text(ops, 52.0, 160.0, 11.0, "Income", false, rgb("6E8B74"));
    text(ops, 140.0, 160.0, 11.0, "Expenses", false, rgb("A56B66"));
    text(ops, 240.0, 160.0, 11.0, "Net", false, rgb("B8A48A"));
}

fn line_series(ops: &mut String, values: &[f64], left: f64, right: f64, bottom: f64, top: f64, max: f64, color: (f64, f64, f64)) {
    if values.is_empty() {
        return;
    }
    let step = if values.len() == 1 { 0.0 } else { (right - left) / (values.len() - 1) as f64 };
    ops.push_str(&format!("{:.3} {:.3} {:.3} RG\n", color.0, color.1, color.2));
    ops.push_str("1.6 w\n");
    for (index, value) in values.iter().enumerate() {
        let x = left + step * index as f64;
        let y = bottom + (value.abs() / max) * (top - bottom);
        if index == 0 {
            ops.push_str(&format!("{x:.2} {y:.2} m\n"));
        } else {
            ops.push_str(&format!("{x:.2} {y:.2} l\n"));
        }
    }
    ops.push_str("S\n");
}

fn wedge(ops: &mut String, cx: f64, cy: f64, outer: f64, inner: f64, start: f64, end: f64, color: &str) {
    let sweep = (end - start).clamp(0.4, 360.0);
    let steps = (sweep / 8.0).ceil().max(2.0) as i32;
    let (r, g, b) = parse_color(color);
    ops.push_str(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
    let (x0, y0) = polar(cx, cy, inner, start);
    ops.push_str(&format!("{x0:.2} {y0:.2} m\n"));
    for step in 0..=steps {
        let angle = start + sweep * (step as f64 / steps as f64);
        let (x, y) = polar(cx, cy, outer, angle);
        ops.push_str(&format!("{x:.2} {y:.2} l\n"));
    }
    for step in (0..=steps).rev() {
        let angle = start + sweep * (step as f64 / steps as f64);
        let (x, y) = polar(cx, cy, inner, angle);
        ops.push_str(&format!("{x:.2} {y:.2} l\n"));
    }
    ops.push_str("h f\n");
}

fn polar(cx: f64, cy: f64, radius: f64, degrees: f64) -> (f64, f64) {
    let radians = degrees.to_radians();
    (cx + radius * radians.cos(), cy + radius * radians.sin())
}

fn disk(ops: &mut String, cx: f64, cy: f64, radius: f64, color: (f64, f64, f64)) {
    wedge(ops, cx, cy, radius, 0.0, 0.0, 359.5, &format!("#{:02X}{:02X}{:02X}", (color.0 * 255.0) as u8, (color.1 * 255.0) as u8, (color.2 * 255.0) as u8));
}

fn swatch(ops: &mut String, x: f64, y: f64, color: &str) {
    rect(ops, x, y, 10.0, 10.0, parse_color(color));
}

fn rect(ops: &mut String, x: f64, y: f64, w: f64, h: f64, color: (f64, f64, f64)) {
    ops.push_str(&format!("{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re f\n", color.0, color.1, color.2, x, y, w, h));
}

fn stroke_rect(ops: &mut String, x: f64, y: f64, w: f64, h: f64, color: (f64, f64, f64), width: f64) {
    ops.push_str(&format!("{:.3} {:.3} {:.3} RG\n{width:.2} w\n{x:.2} {y:.2} {w:.2} {h:.2} re S\n", color.0, color.1, color.2));
}

fn text(ops: &mut String, x: f64, y: f64, size: f64, value: &str, bold: bool, color: (f64, f64, f64)) {
    let font = if bold { "F2" } else { "F1" };
    ops.push_str(&format!(
        "BT /{font} {size:.1} Tf {:.3} {:.3} {:.3} rg 1 0 0 1 {x:.2} {y:.2} Tm ({}) Tj ET\n",
        color.0,
        color.1,
        color.2,
        pdf_text(value)
    ));
}

fn clip(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn pdf_text(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        let byte = winansi(ch);
        match byte {
            b'\\' | b'(' | b')' => {
                out.push('\\');
                out.push(byte as char);
            }
            32..=126 => out.push(byte as char),
            _ => out.push_str(&format!("\\{byte:03o}")),
        }
    }
    out
}

fn winansi(ch: char) -> u8 {
    match ch {
        '€' => b'E',
        '—' | '–' => b'-',
        '’' | '‘' => b'\'',
        '“' | '”' => b'"',
        '…' => b'.',
        '¡' => 0xA1,
        '¿' => 0xBF,
        '·' => 0xB7,
        'ª' => 0xAA,
        'º' => 0xBA,
        'À' => 0xC0,
        'Á' => 0xC1,
        'Â' => 0xC2,
        'Ã' => 0xC3,
        'Ä' => 0xC4,
        'Å' => 0xC5,
        'Æ' => 0xC6,
        'Ç' => 0xC7,
        'È' => 0xC8,
        'É' => 0xC9,
        'Ê' => 0xCA,
        'Ë' => 0xCB,
        'Ì' => 0xCC,
        'Í' => 0xCD,
        'Î' => 0xCE,
        'Ï' => 0xCF,
        'Ñ' => 0xD1,
        'Ò' => 0xD2,
        'Ó' => 0xD3,
        'Ô' => 0xD4,
        'Õ' => 0xD5,
        'Ö' => 0xD6,
        'Ù' => 0xD9,
        'Ú' => 0xDA,
        'Û' => 0xDB,
        'Ü' => 0xDC,
        'ß' => 0xDF,
        'à' => 0xE0,
        'á' => 0xE1,
        'â' => 0xE2,
        'ã' => 0xE3,
        'ä' => 0xE4,
        'å' => 0xE5,
        'æ' => 0xE6,
        'ç' => 0xE7,
        'è' => 0xE8,
        'é' => 0xE9,
        'ê' => 0xEA,
        'ë' => 0xEB,
        'ì' => 0xEC,
        'í' => 0xED,
        'î' => 0xEE,
        'ï' => 0xEF,
        'ñ' => 0xF1,
        'ò' => 0xF2,
        'ó' => 0xF3,
        'ô' => 0xF4,
        'õ' => 0xF5,
        'ö' => 0xF6,
        'ù' => 0xF9,
        'ú' => 0xFA,
        'û' => 0xFB,
        'ü' => 0xFC,
        'ÿ' => 0xFF,
        other if other.is_ascii() => other as u8,
        _ => b'?',
    }
}

fn rgb(hex: &str) -> (f64, f64, f64) {
    parse_color(&format!("#{hex}"))
}

fn parse_color(value: &str) -> (f64, f64, f64) {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return (0.72, 0.64, 0.54);
    }
    let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).unwrap_or(180) as f64 / 255.0;
    (channel(0), channel(2), channel(4))
}

fn assemble(streams: &[String]) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let page_count = streams.len();
    let font_regular = objects.len() + 3 + page_count * 2;
    let font_bold = font_regular + 1;
    objects.push(format!("<< /Type /Catalog /Pages 2 0 R >>").into_bytes());
    let mut kids = String::from("[");
    for index in 0..page_count {
        let page_id = 3 + index * 2;
        kids.push_str(&format!(" {page_id} 0 R"));
    }
    kids.push_str(" ]");
    objects.push(format!("<< /Type /Pages /Count {page_count} /Kids {kids} >>").into_bytes());
    for (index, stream) in streams.iter().enumerate() {
        let page_id = 3 + index * 2;
        let content_id = page_id + 1;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W:.2} {H:.2}] /Contents {content_id} 0 R /Resources << /Font << /F1 {font_regular} 0 R /F2 {font_bold} 0 R >> >> >>"
            )
            .into_bytes(),
        );
        let bytes = stream.as_bytes();
        let mut body = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\nendstream");
        objects.push(body);
    }
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0u32];
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len() as u32;
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::pdf_text;

    #[test]
    fn winansi_keeps_distinct_letters() {
        assert_eq!(pdf_text("ä"), "\\344");
        assert_eq!(pdf_text("ö"), "\\366");
        assert_eq!(pdf_text("ü"), "\\374");
        assert_eq!(pdf_text("Ä"), "\\304");
        assert_eq!(pdf_text("é"), "\\351");
        assert_eq!(pdf_text("à"), "\\340");
        assert_eq!(pdf_text("ñ"), "\\361");
        assert_eq!(pdf_text("ç"), "\\347");
        assert_eq!(pdf_text("ß"), "\\337");
        assert_eq!(pdf_text("¿"), "\\277");
    }
}
