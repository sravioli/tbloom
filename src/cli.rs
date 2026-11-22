// Command-line options.
use clap::Parser;
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};

// Help colors (clap drops them when the output is not a terminal, or with NO_COLOR set).
const HEADER: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);
const LITERAL: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
const STYLES: Styles = Styles::styled()
    .header(HEADER)
    .usage(HEADER)
    .literal(LITERAL)
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(LITERAL)
    .invalid(AnsiColor::Yellow.on_default());

/// The controls, styled like the option list above them.
fn controls() -> String {
    let (h, k) = (HEADER, LITERAL);
    let key = |s: &str| format!("{k}{s}{k:#}");
    format!(
        "{h}Controls:{h:#} (each rotates about one fixed screen axis)
  {tab} or {n}       pick what a mouse drag does: 1 turn (left/right), 2 tilt (up/down), 3 roll
  {ad}, {lr}    turn          {ws}, {ud}    tilt          {sad} (shift)    roll
  mouse wheel zooms, {r} resets the view, {q} or {cc} quits",
        tab = key("Tab"),
        n = key("1/2/3"),
        ad = key("a/d"),
        lr = key("left/right"),
        ws = key("w/s"),
        ud = key("up/down"),
        sad = key("A/D"),
        r = key("r"),
        q = key("q"),
        cc = key("Ctrl-C"),
    )
}

/// Show a 3D bouquet in the terminal. Turn it with the mouse or the keyboard.
#[derive(Parser)]
#[command(name = "tbloom", after_help = controls(), styles = STYLES, args_override_self = true)]
pub struct Opt {
    /// Show a binary FBX model (textures looked up next to it or in ../textures)
    #[arg(short, long, value_name = "FILE")]
    pub model: Option<String>,
    /// Sextant glyphs (2x3 px per cell); needs a font with Legacy Computing blocks
    #[arg(short, long, conflicts_with = "ascii")]
    pub nerd: bool,
    /// Plain text: ASCII luminance ramp (still colored unless --no-color)
    #[arg(short, long)]
    pub ascii: bool,
    /// No color escapes (monochrome): luminance ramp, or dithered sextants with --nerd
    #[arg(short = 'M', long)]
    pub no_color: bool,
    /// Anti-aliasing: redraw a supersampled frame once input goes quiet
    #[arg(short = 'A', long)]
    pub aa: bool,
    /// Print one W x H frame to stdout and exit
    #[arg(short, long = "snapshot", value_name = "WxH", value_parser = size)]
    pub snap: Option<(usize, usize)>,
}

/// "WxH" with both sides > 0.
fn size(s: &str) -> Result<(usize, usize), String> {
    s.split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .filter(|&(w, h)| w > 0 && h > 0)
        .ok_or_else(|| format!("expected WxH with W, H > 0, got {s:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn options() {
        Opt::command().debug_assert();
        assert_eq!(size("80x24"), Ok((80, 24)));
        assert!(size("0x24").is_err() && size("80").is_err() && size("ax2").is_err());
        let o = Opt::try_parse_from(["tbloom", "--nerd", "--snapshot", "10x5"]).unwrap();
        assert!(o.nerd && !o.ascii && o.snap == Some((10, 5)));
        assert!(Opt::try_parse_from(["tbloom", "--nerd", "--ascii"]).is_err());
        let o =
            Opt::try_parse_from(["tbloom", "-n", "-M", "-A", "-s", "4x2", "-m", "f.fbx"]).unwrap();
        assert!(
            o.nerd
                && o.no_color
                && o.aa
                && o.snap == Some((4, 2))
                && o.model.as_deref() == Some("f.fbx")
        );
        assert!(Opt::try_parse_from(["tbloom", "-a", "-n"]).is_err());
    }
}
