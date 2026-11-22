// Parse raw stdin bytes into events. Returns events, leftover (incomplete escape sequence).

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    Byte(u8),
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// SGR 1006 encoding: button code, column, row, press/motion (true) or release
    Mouse {
        b: i64,
        x: i64,
        y: i64,
        down: bool,
    },
    Key(Key),
}

fn num(s: &[u8]) -> i64 {
    s.iter().fold(0i64, |n, d| {
        n.saturating_mul(10).saturating_add((d - b'0') as i64)
    })
}

/// `b;x;yM` or `b;x;ym` at the start of s: the event and its length.
fn sgr_mouse(s: &[u8]) -> Option<(Event, usize)> {
    let mut v = [0; 3];
    let mut i = 0;
    for (k, n) in v.iter_mut().enumerate() {
        let len = s[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if len == 0 {
            return None;
        }
        *n = num(&s[i..i + len]);
        i += len;
        let want: &[u8] = if k < 2 { b";" } else { b"Mm" };
        if !s.get(i).is_some_and(|c| want.contains(c)) {
            return None;
        }
        i += 1;
    }
    Some((
        Event::Mouse {
            b: v[0],
            x: v[1],
            y: v[2],
            down: s[i - 1] == b'M',
        },
        i,
    ))
}

pub fn parse(buf: &[u8]) -> (Vec<Event>, Vec<u8>) {
    let (mut ev, mut i, n) = (Vec::new(), 0, buf.len());
    while i < n {
        let c = buf[i];
        if c != 27 {
            ev.push(Event::Key(Key::Byte(c)));
            i += 1;
            continue;
        }
        if i == n - 1 {
            break; // maybe the start of a sequence: wait for more bytes
        }
        if buf[i + 1..].starts_with(b"[<") {
            let rest = &buf[i + 3..];
            if let Some((e, len)) = sgr_mouse(rest) {
                ev.push(e);
                i += 3 + len;
            } else if rest.iter().all(|c| c.is_ascii_digit() || *c == b';') {
                break;
            } else {
                i += 1;
            }
        } else if buf[i + 1] == b'[' {
            let rest = &buf[i + 2..];
            let p = rest
                .iter()
                .take_while(|c| c.is_ascii_digit() || **c == b';' || **c == b'?')
                .count();
            match rest.get(p) {
                Some(&fin) if fin.is_ascii_alphabetic() || fin == b'~' => {
                    let arrow = match fin {
                        b'A' => Some(Key::Up),
                        b'B' => Some(Key::Down),
                        b'C' => Some(Key::Right),
                        b'D' => Some(Key::Left),
                        _ => None,
                    };
                    ev.extend(arrow.map(Event::Key));
                    i += 2 + p + 1;
                }
                None => break,
                Some(_) => i += 1,
            }
        } else {
            ev.push(Event::Key(Key::Byte(c)));
            i += 1;
        }
    }
    (ev, buf[i..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse(e: &Event) -> (i64, i64, i64, bool) {
        match *e {
            Event::Mouse { b, x, y, down } => (b, x, y, down),
            _ => panic!("not a mouse event: {e:?}"),
        }
    }

    #[test]
    fn sgr_mouse_parsing() {
        // several events in one chunk and a split sequence
        let (ev, rest) = parse(b"\x1b[<0;10;5M\x1b[<32;12;6Mq\x1b[<0;12;6m\x1b[<3");
        assert!(ev.len() == 4 && rest == b"\x1b[<3");
        assert_eq!(mouse(&ev[0]), (0, 10, 5, true));
        assert_eq!(mouse(&ev[1]).0, 32);
        assert_eq!(ev[2], Event::Key(Key::Byte(b'q')));
        assert!(!mouse(&ev[3]).3);
        let (ev, rest) = parse(&[rest, b"5;1;1M\x1b[H".to_vec()].concat());
        assert!(ev.len() == 1 && mouse(&ev[0]).0 == 35 && rest.is_empty());
        let (ev, _) = parse(b"\x1b[A\x1b[D\x1b[<64;5;5M");
        assert!(
            ev[0] == Event::Key(Key::Up) && ev[1] == Event::Key(Key::Left) && mouse(&ev[2]).0 == 64
        );
    }
}
