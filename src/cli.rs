// Command-line options.
use clap::Parser;

const CONTROLS: &str = "\
Controls (each rotates about one fixed screen axis):
  Tab or 1/2/3   pick what a mouse drag does: 1 turn (left/right), 2 tilt (up/down), 3 roll
  a/d, left/right    turn          w/s, up/down    tilt          A/D (shift)    roll
  mouse wheel zooms, r resets the view, q or Ctrl-C quits";

/// Fullscreen terminal renderer for a 3D autumn bouquet.
#[derive(Parser)]
#[command(name = "tbloom", after_help = CONTROLS, args_override_self = true)]
pub struct Opt {
    /// Show a binary FBX model (textures looked up next to it or in ../textures)
    #[arg(long, value_name = "FILE")]
    pub model: Option<String>,
    /// Sextant glyphs (2x3 px per cell); needs a font with Legacy Computing blocks
    #[arg(long, conflicts_with = "ascii")]
    pub nerd: bool,
    /// Plain text: ASCII luminance ramp (still colored unless --no-color)
    #[arg(long)]
    pub ascii: bool,
    /// No color escapes: luminance ramp (or dithered sextants with --nerd)
    #[arg(long)]
    pub no_color: bool,
    /// Anti-aliasing: redraw a supersampled frame once input goes quiet
    #[arg(long)]
    pub aa: bool,
    /// Print one W x H frame to stdout and exit
    #[arg(long = "snapshot", value_name = "WxH", value_parser = size)]
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
    }
}
