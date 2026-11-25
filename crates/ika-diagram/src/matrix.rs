//! `matrix`: 2×2。`x:`・`y:` の軸を「低い側 -> 高い側」で書き、`右上`・`左上`・`右下`・`左下` の
//! 象限に項目を `,`（全角の `、` も）で区切って置く。象限には `| 名前` を付けられる。`*` は主役の項目。
//! 象限の中の位置は決めない（書いた順に並べる）。省いた象限は空のまま描く。
//!
//! ```text
//! x: 数字で競う -> 体験で選ぶ
//! y: その場かぎり -> 積み上がる
//! 右上 | 狙う場所: *photox
//! 左上: Instagram, X
//! ```

use crate::{Line, ParseError, esc, split_label, split_note};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Matrix {
    pub x: Axis,
    pub y: Axis,
    /// 左上・右上・左下・右下の順（描く順）
    pub quadrants: [Quadrant; 4],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Axis {
    pub low: String,
    pub high: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Quadrant {
    pub label: String,
    pub items: Vec<MatrixItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixItem {
    pub name: String,
    /// `*` を付けた主役の項目
    pub key: bool,
}

/// 象限の名前。[`Matrix::quadrants`] の並びと同じ
const QUADRANTS: [&str; 4] = ["左上", "右上", "左下", "右下"];

pub(crate) fn parse(lines: &[Line]) -> Result<Matrix, Vec<ParseError>> {
    let mut matrix = Matrix::default();
    let mut errors = Vec::new();
    let (mut has_x, mut has_y) = (false, false);
    let mut seen = [false; 4];
    for line in lines {
        let result = (|| {
            let (head, body) = split_label(line.text)
                .ok_or("「x: 低い側 -> 高い側」か「右上: 項目, 項目」の形で書きます")?;
            let (key, label) = split_note(head);
            match key {
                "x" | "X" | "y" | "Y" => {
                    let is_x = key.eq_ignore_ascii_case("x");
                    let has = if is_x { &mut has_x } else { &mut has_y };
                    if *has {
                        return Err(format!("{key} 軸は1行だけです"));
                    }
                    *has = true;
                    let axis = axis(body)?;
                    if is_x {
                        matrix.x = axis;
                    } else {
                        matrix.y = axis;
                    }
                    Ok(())
                }
                _ => {
                    let at = QUADRANTS.iter().position(|q| *q == key).ok_or_else(|| {
                        format!("象限は 右上・左上・右下・左下 のどれかで書きます: {key}")
                    })?;
                    if seen[at] {
                        return Err(format!("象限「{key}」は1行だけです"));
                    }
                    seen[at] = true;
                    matrix.quadrants[at] = Quadrant {
                        label: label.to_string(),
                        items: items(body),
                    };
                    Ok(())
                }
            }
        })();
        if let Err(message) = result {
            errors.push(ParseError::new(line.no, message));
        }
    }
    for (has, name) in [(has_x, "x"), (has_y, "y")] {
        if !has {
            errors.push(ParseError::new(
                1,
                format!("{name} 軸（「{name}: 低い側 -> 高い側」）がありません"),
            ));
        }
    }
    if !errors.is_empty() {
        errors.sort_by_key(|e| e.line);
        return Err(errors);
    }
    Ok(matrix)
}

fn axis(text: &str) -> Result<Axis, String> {
    let message = "軸は「低い側 -> 高い側」と書きます";
    let (low, high) = text.split_once("->").ok_or(message)?;
    let (low, high) = (low.trim(), high.trim());
    if low.is_empty() || high.is_empty() || high.contains("->") {
        return Err(message.into());
    }
    Ok(Axis {
        low: low.to_string(),
        high: high.to_string(),
    })
}

fn items(text: &str) -> Vec<MatrixItem> {
    text.split([',', '、'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| match t.strip_prefix('*') {
            Some(name) => MatrixItem {
                name: name.trim().to_string(),
                key: true,
            },
            None => MatrixItem {
                name: t.to_string(),
                key: false,
            },
        })
        .filter(|item| !item.name.is_empty())
        .collect()
}

pub(crate) fn render(matrix: &Matrix) -> String {
    let mut out = format!(
        "<div class=\"ikd-mx\"><div class=\"ikd-mx-y\"><span>{}</span><span>{}</span></div><div class=\"ikd-mx-grid\">",
        esc(&matrix.y.high),
        esc(&matrix.y.low)
    );
    for quadrant in &matrix.quadrants {
        out.push_str("<div class=\"ikd-mx-cell\">");
        if !quadrant.label.is_empty() {
            out.push_str(&format!("<b>{}</b>", esc(&quadrant.label)));
        }
        if !quadrant.items.is_empty() {
            out.push_str("<ul>");
            for item in &quadrant.items {
                let class = if item.key { " class=\"ikd-key\"" } else { "" };
                out.push_str(&format!("<li{class}>{}</li>", esc(&item.name)));
            }
            out.push_str("</ul>");
        }
        out.push_str("</div>");
    }
    out.push_str(&format!(
        "</div><div class=\"ikd-mx-x\"><span>{}</span><span>{}</span></div></div>",
        esc(&matrix.x.low),
        esc(&matrix.x.high)
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines;

    fn parse_src(src: &str) -> Result<Matrix, Vec<ParseError>> {
        parse(&lines(src))
    }

    fn item(name: &str, key: bool) -> MatrixItem {
        MatrixItem {
            name: name.into(),
            key,
        }
    }

    #[test]
    fn parses_axes_quadrants_labels_and_key_items() {
        let matrix = parse_src(
            "x: 数字 -> 体験\ny：一回 -> 積み上がる\n右上 | 狙う場所: *photox\n左上: Instagram, X、 *Y\n右下:\n",
        )
        .unwrap();
        assert_eq!(matrix.x.low, "数字");
        assert_eq!(matrix.x.high, "体験");
        assert_eq!(matrix.y.high, "積み上がる");
        let [top_left, top_right, bottom_left, bottom_right] = &matrix.quadrants;
        assert_eq!(top_right.label, "狙う場所");
        assert_eq!(top_right.items, [item("photox", true)]);
        assert_eq!(
            top_left.items,
            [item("Instagram", false), item("X", false), item("Y", true)]
        );
        assert_eq!(*bottom_left, Quadrant::default());
        assert!(bottom_right.items.is_empty());
    }

    #[test]
    fn rejects_missing_or_bad_axes_unknown_and_repeated_quadrants() {
        let errors = parse_src("x: a -> b\n右上: a").unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("y 軸"), "{errors:?}");

        let errors =
            parse_src("x: a\ny: a -> b -> c\n真ん中: a\n右上: a\n右上: b\nx: a -> b\n: a\nだけ")
                .unwrap_err();
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [1, 2, 3, 5, 6, 7, 8], "{errors:?}");
    }

    #[test]
    fn renders_quadrants_in_their_cells_with_axis_labels() {
        let html = render(
            &parse_src("x: 左 -> 右\ny: 下 -> 上\n右下: d\n左上 | 名: a\n右上: *b\n左下: c")
                .unwrap(),
        );
        assert_eq!(
            html,
            "<div class=\"ikd-mx\"><div class=\"ikd-mx-y\"><span>上</span><span>下</span></div>\
             <div class=\"ikd-mx-grid\">\
             <div class=\"ikd-mx-cell\"><b>名</b><ul><li>a</li></ul></div>\
             <div class=\"ikd-mx-cell\"><ul><li class=\"ikd-key\">b</li></ul></div>\
             <div class=\"ikd-mx-cell\"><ul><li>c</li></ul></div>\
             <div class=\"ikd-mx-cell\"><ul><li>d</li></ul></div>\
             </div><div class=\"ikd-mx-x\"><span>左</span><span>右</span></div></div>"
        );
    }
}
