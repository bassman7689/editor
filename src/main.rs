extern crate libc;

use libc::{c_int, c_ulong};
use libc::termios as Termios;
use libc::winsize as Winsize;
use std::{io, mem};
use std::io::{Read, Write};

const EDITOR_VERSION: &str = "0.0.1";

trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

macro_rules! impl_is_minus_one {
    ($($t:ident)*) => ($(impl IsMinusOne for $t {
        fn is_minus_one(&self) -> bool {
            *self == -1
        }
    })*)
}

impl_is_minus_one! { i8 i16 i32 i64 isize }

fn cvt<T: IsMinusOne>(t: T) -> io::Result<T> {
    if t.is_minus_one() {
        Err(io::Error::last_os_error())
    } else {
        Ok(t)
    }
}

pub fn get_terminal_attr() -> io::Result<Termios> {
    extern "C" {
        pub fn tcgetattr(fd: c_int, termptr: *mut Termios) -> c_int;
    }
    unsafe {
        let mut termios = mem::zeroed();
        cvt(tcgetattr(libc::STDIN_FILENO, &mut termios))?;
        Ok(termios)
    }
}

pub fn set_terminal_attr(termios: &Termios) -> io::Result<()> {
    extern "C" {
        pub fn tcsetattr(fd: c_int, opt: c_int, termptr: *const Termios) -> c_int;
    }
    cvt(unsafe { tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios) }).and(Ok(()))
}

fn enable_raw_mode() -> io::Result<Termios> {
    let orig_termios = get_terminal_attr()?;

    let mut new_termios = orig_termios;
    new_termios.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
    new_termios.c_oflag &= !(libc::OPOST);
    new_termios.c_cflag |= libc::CS8;
    new_termios.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
    set_terminal_attr(&new_termios)?;

    Ok(orig_termios)
}

fn disable_raw_mode(orig_termios: &Termios) -> io::Result<()> {
    set_terminal_attr(orig_termios)
}

struct Terminal {
    stdin: io::Bytes<io::Stdin>,
    stdout: io::Stdout,
    orig_termios: Termios,
}

enum Key {
    Char(char),
    Ctrl(char),
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    PageUp,
    PageDown,
}

impl Terminal {
    fn new() -> Terminal {
        let mut stdout = io::stdout();
        let orig_termios = enable_raw_mode().unwrap();
        write!(stdout, "{}", "\x1b[?1049h").unwrap();

        Terminal{
            stdin: io::stdin().bytes(),
            stdout,
            orig_termios,
        }
    }

    // TODO(sean): Clean up this damn code!!!
    fn read_key(&mut self) -> io::Result<Option<Key>> {
        if let Some(c) = self.stdin.next() {
            let c = c?;
            match c {
                b'\x1b' => {
                    if let Some(c) = self.stdin.next() {
                        let c = c?;
                        if c == b'[' {
                            if let Some(c) = self.stdin.next() {
                                let c = c?;
                                match c {
                                    b'0'..=b'9' => {
                                        if let Some(cb) = self.stdin.next() {
                                            let cb = cb?;
                                            if cb == b'~' {
                                                match c {
                                                    b'5' => Ok(Some(Key::PageUp)),
                                                    b'6' => Ok(Some(Key::PageDown)),
                                                    _ => Ok(Some(Key::Char('\x1b'))),
                                                }
                                            } else {
                                                Ok(Some(Key::Char('\x1b')))
                                            }
                                        } else {
                                            Ok(Some(Key::Char('\x1b')))
                                        }
                                    },
                                    b'A' => Ok(Some(Key::ArrowUp)),
                                    b'B' => Ok(Some(Key::ArrowDown)),
                                    b'C' => Ok(Some(Key::ArrowRight)),
                                    b'D' => Ok(Some(Key::ArrowLeft)),
                                    _ => Ok(Some(Key::Char('\x1b')))
                                }
                            } else {
                                Ok(Some(Key::Char('\x1b')))
                            }
                        } else {
                            Ok(Some(Key::Char('\x1b')))
                        }
                    } else {
                        Ok(Some(Key::Char('\x1b')))
                    }
                },
                b'\x01'..=b'\x1A' => Ok(Some(Key::Ctrl((c as u8 - 0x1 + b'a') as char))),
                0..=127 => Ok(Some(Key::Char(c as char))),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn get_size(&mut self) -> io::Result<(u16, u16)> {
        if let Ok(size) = self.get_size_ioctl() {
            Ok(size)
        } else {
          self.get_size_escape_codes()
        }
    }

    fn get_size_ioctl(&mut self) -> io::Result<(u16, u16)> {
        extern "C" {
            pub fn ioctl(fd: c_int, opt: c_ulong, ws: *mut Winsize) -> c_int;
        }

            let ws = unsafe {
            let mut ws = mem::zeroed();
            cvt(ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws))?;
            ws
        };

        Ok((ws.ws_row, ws.ws_col))
    }

    fn get_cursor_position(&mut self) -> io::Result<(u16, u16)> {
        let mut buf = Vec::<u8>::new();

        write!(self, "{}", "\x1b[6n")?;
        write!(self, "{}", "\r\n")?;

        loop {
            buf.push(self.stdin.next().unwrap().unwrap());
            if buf[buf.len() - 1] == b'R' {
                break;
            }
        }

        let mut size = (0, 0);
        if buf[0] != b'\x1b' || buf[1] != b'[' {
            Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected data in stdin"))
        } else {
            let first = buf.iter().skip(2).take_while(|x| **x != b';').map(|x| *x).collect::<Vec<u8>>();
            let second = buf.iter().skip(first.len() + 3).map(|x| *x).collect::<Vec<u8>>();

            size.0 = first
                .into_iter()
                .map(|i| i.to_string())
                .collect::<String>()
                .parse::<u16>()
                .or_else(|_| Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected data in stdin")))?;

            size.1 = second
                .into_iter()
                .skip(1)
                .take_while(|i| (*i as char).is_digit(10))
                .map(|i| i.to_string())
                .collect::<String>()
                .parse::<u16>()
                .or_else(|_| Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected data in stdin")))?;
                                 Ok(size)
        }
    }

    fn get_size_escape_codes(&mut self) -> io::Result<(u16, u16)> {
        write!(self, "{}", "\x1b[999C\x1b[999B")?;
        let pos = self.get_cursor_position();
        write!(self, "{}", "\x1b[2J")?;
        write!(self, "{}", "\x1b[H")?;
        pos
    }
}

impl Write for Terminal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

struct Editor {
    term: Terminal,
    cx: i32,
    cy: i32,
    screen_rows: u16,
    screen_cols: u16,
}

impl Editor {
    fn new() -> Editor {
        let mut term = Terminal::new();
        let (rows, cols) = term.get_size().unwrap();

        Editor {
            cx: 0,
            cy: 0,
            term: term,
            screen_rows: rows,
            screen_cols: cols,
        }
    }

    fn draw_welcome_message(&mut self, buf: &mut AppendBuffer) -> io::Result<()> {
        let welcome = format!("Editor -- version {}", EDITOR_VERSION);
        let welcome_len = if welcome.len() > self.screen_cols as usize {
            self.screen_cols as usize
        } else {
            welcome.len()
        };
        let mut padding = (self.screen_cols as usize - welcome_len) / 2;
        if padding > 0 {
            write!(buf, "{}", "~")?;
            padding -= 1;
        }

        while padding > 0 {
            write!(buf, "{}", " ")?;
            padding -= 1;
        }

        write!(buf, "{}", &welcome[..welcome_len])
    }

    fn draw_rows(&mut self, buf: &mut AppendBuffer) -> io::Result<()> {
        for y in 0..self.screen_rows {
            if y == self.screen_rows / 3  {
                self.draw_welcome_message(buf)?;
            } else {
                write!(buf, "{}", "~")?;
            }

            write!(buf, "{}", "\x1b[K")?;
            if y < self.screen_rows - 1 {
                write!(buf, "{}", "\r\n")?;
            }
        }

        Ok(())
    }

    fn refresh_screen(&mut self) -> io::Result<()> {
        let mut buf = AppendBuffer::new();

        write!(buf, "{}", "\x1b[?25l")?;
        write!(buf, "{}", "\x1b[H")?;

        self.draw_rows(&mut buf)?;

        let cursor_move = format!("\x1b[{};{}H", self.cy + 1, self.cx + 1);
        write!(buf, "{}", cursor_move)?;

        write!(buf, "{}", "\x1b[?25h")?;

        self.term.write(&buf.bytes[..])?;
        self.term.flush()
    }

    fn move_cursor(&mut self, key: Key) {
        match key {
            Key::ArrowLeft => {
                if self.cx != 0 {
                    self.cx -= 1;
                }
            },
            Key::ArrowRight => {
                if self.cx != self.screen_cols as i32 - 1 {
                    self.cx += 1;
                }
            },
            Key::ArrowUp => {
                if self.cy != 0 {
                    self.cy -= 1;
                }
            },
            Key::ArrowDown => {
                if self.cy != self.screen_rows as i32 - 1 {
                    self.cy += 1;
                }
            },
            _ => {}
        }
    }

    fn handle_input(&mut self) -> io::Result<()> {
        if let Some(c) = self.term.read_key()? {
            match c {
                Key::Ctrl('q') => {
                    write!(self.term, "{}", "\x1b[H")?;
                    write!(self.term, "{}", "\x1b[2J")?;
                    write!(self.term, "{}", "\x1b[?1049l")?;
                    self.term.flush()?;
                    disable_raw_mode(&self.term.orig_termios)?;
                    std::process::exit(0);
                },
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => {
                    self.move_cursor(c);
                    Ok(())
                },
                Key::PageUp => {
                    let times = self.screen_rows;
                    for _ in 0..times {
                        self.move_cursor(Key::ArrowUp);
                    }
                    Ok(())
                }
                Key::PageDown => {
                    let times = self.screen_rows;
                    for _ in 0..times {
                        self.move_cursor(Key::ArrowDown);
                    }
                    Ok(())
                },
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }
}

struct AppendBuffer {
    bytes: Vec<u8>,
}

impl AppendBuffer {
    fn new() -> Self {
        AppendBuffer{
            bytes: vec![],
        }
    }
}

impl Write for AppendBuffer {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for v in b.iter() {
            self.bytes.push(*v);
        }
        Ok(b.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() {
    let mut editor = Editor::new();

    loop {
        editor.refresh_screen().unwrap();
        editor.handle_input().unwrap();
    }
}
