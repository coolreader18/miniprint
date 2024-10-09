use escpos::driver::ConsoleDriver;
use escpos::printer::Printer;
use escpos::printer_options::PrinterOptions;
use escpos::utils::Protocol;
use jiff::civil::Date;
use resvg::{tiny_skia, usvg};
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MiniCrossword {
    body: Vec<Puzzle>,
    constructors: Vec<String>,
    editor: String,
    publication_date: Date,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Puzzle {
    board: String,
    clue_lists: Vec<ClueList>,
    clues: Vec<Clue>,
}

#[derive(Deserialize)]
struct ClueList {
    clues: Vec<u16>,
    name: Direction,
}

#[derive(Deserialize, Debug)]
enum Direction {
    Across,
    Down,
}

#[derive(Deserialize)]
struct Clue {
    // cells: Vec<u16>,
    // direction: Direction,
    label: String,
    text: Vec<ClueText>,
}

#[derive(Deserialize)]
struct ClueText {
    plain: String,
    // formatted: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chars_per_line = 32;
    let pixels_per_char = 12u8;
    let dpi = 203.0;

    let wrap_opts = || textwrap::Options::new(chars_per_line.into());

    let mini: MiniCrossword =
        ureq::get("https://www.nytimes.com/svc/crosswords/v6/puzzle/mini.json")
            .set("User-Agent", "miniprinter")
            .call()?
            .into_json()?;

    let puzzle = &mini.body[0];

    let mut opt = usvg::Options {
        dpi,
        shape_rendering: usvg::ShapeRendering::CrispEdges,
        ..Default::default()
    };
    opt.fontdb_mut().load_system_fonts();
    let svg = usvg::Tree::from_str(&puzzle.board, &opt)?;
    let size = svg.size();

    let target_width = f32::from(pixels_per_char) * f32::from(chars_per_line);

    let scale = target_width / svg.size().width();
    // canvas width should be the full width of the paper, but the render transform is rounded
    // to an even-ish number so that lines don't get lost rendering to the low resolution
    let scale = (scale * 100.0).round() / 100.0;
    let canvas_size = usvg::Size::from_wh(target_width, size.height() * scale)
        .unwrap()
        .to_int_size();
    let trans = usvg::Transform::from_scale(scale, scale);

    let mut buf = tiny_skia::Pixmap::new(canvas_size.width(), canvas_size.height()).unwrap();
    resvg::render(&svg, trans, &mut buf.as_mut());
    let png = buf.encode_png()?;

    let mut printer = Printer::new(
        ConsoleDriver::open(true),
        Protocol::default(),
        Some(PrinterOptions::new(None, None, chars_per_line)),
    );
    printer.writeln("The NYT Mini Crossword")?;
    printer
        .writeln(&mini.publication_date.strftime("%A, %B %-d, %Y").to_string())?
        .feed()?;

    printer.bit_image_from_bytes(&png)?;

    printer.feed()?.feed()?;

    let write_wrapped = |printer: &mut Printer<_>, text, opts: textwrap::Options<'_>| {
        let text = textwrap::wrap(text, opts);
        text.into_iter()
            .try_for_each(|line| printer.writeln(&line).map(drop))
    };

    for clues in &puzzle.clue_lists {
        printer.writeln(&format!("{:?}:", clues.name))?;
        for &clue_num in &clues.clues {
            let clue = &puzzle.clues[clue_num as usize];
            let label = format!("{}: ", clue.label);
            write_wrapped(
                &mut printer,
                &clue.text[0].plain,
                wrap_opts()
                    .initial_indent(&label)
                    .subsequent_indent(&" ".repeat(label.width())),
            )?;
        }
        printer.feed()?;
    }

    write_wrapped(
        &mut printer,
        &format_list(&mini.constructors),
        wrap_opts().initial_indent("By ").subsequent_indent("   "),
    )?;
    printer.write("Edited by ")?.writeln(&mini.editor)?;

    printer.print_cut()?;

    Ok(())
}

fn format_list(s: &[String]) -> String {
    match s {
        [x] => x.clone(),
        [x, y] => [x, " and ", y].concat(),
        xs => {
            let mut out = String::new();
            for (i, x) in xs.iter().enumerate() {
                if i == xs.len() - 1 {
                    out.push_str(", and ");
                } else if i != 0 {
                    out.push_str(", ");
                }
                out.push_str(x);
            }
            out
        }
    }
}
